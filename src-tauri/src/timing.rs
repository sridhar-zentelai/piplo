//! TEMPORARY latency instrumentation. Delete this module (and its call sites)
//! once the measurement run is done.
//!
//! Ported from saylo's `timing.rs` so the two apps' numbers are directly
//! comparable: same stage boundaries, same rule that the stages sum exactly to
//! the total, same CSV shape. Only the names differ — `grammar` here for what
//! saylo calls `qwen`, since that is what the module is called on this side.
//!
//! Behaviour-neutral by construction: every entry point takes a mark and returns,
//! and every failure path is swallowed. Nothing here can change what gets
//! transcribed, cleaned or typed.
//!
//! One dictation runs at a time — `session::start` refuses a second while the
//! widget is expanded — so a single global slot is enough. A run that is
//! cancelled or fails is simply overwritten by the next `begin()`.

use std::io::Write;
use std::sync::Mutex;
use std::time::Instant;

/// Where the CSV lands. Fixed to the repo root so the numbers are easy to find.
const LOG: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../piplo-latency.csv");

#[derive(Debug)]
struct Marks {
    /// Key release / ✓ — the moment recording stops.
    t0: Instant,
    wav_ready: Option<Instant>,
    whisper_sent: Option<Instant>,
    whisper_done: Option<Instant>,
    /// Both stay `None` when grammar is skipped or disabled; the report then
    /// counts the stage as 0 and the decision time falls into Gap.
    grammar_sent: Option<Instant>,
    grammar_done: Option<Instant>,
    audio_secs: f64,
    words: usize,
    cleaned: bool,
}

static MARKS: Mutex<Option<Marks>> = Mutex::new(None);

fn with<F: FnOnce(&mut Marks)>(f: F) {
    if let Ok(mut slot) = MARKS.lock() {
        if let Some(marks) = slot.as_mut() {
            f(marks);
        }
    }
}

/// Recording has just been stopped. Starts the clock for `Total`.
pub fn begin(audio_secs: f64) {
    if let Ok(mut slot) = MARKS.lock() {
        *slot = Some(Marks {
            t0: Instant::now(),
            wav_ready: None,
            whisper_sent: None,
            whisper_done: None,
            grammar_sent: None,
            grammar_done: None,
            audio_secs,
            words: 0,
            cleaned: false,
        });
    }
}

/// Resample + WAV encode finished.
pub fn wav_ready() {
    with(|m| m.wav_ready = Some(Instant::now()));
}

/// Immediately before the multipart POST is handed to reqwest.
pub fn whisper_sent() {
    with(|m| m.whisper_sent = Some(Instant::now()));
}

/// Whisper's response fully read.
pub fn whisper_done() {
    with(|m| m.whisper_done = Some(Instant::now()));
}

/// Immediately before the chat-completion POST.
pub fn grammar_sent() {
    with(|m| m.grammar_sent = Some(Instant::now()));
}

/// Grammar's response fully read.
pub fn grammar_done() {
    with(|m| m.grammar_done = Some(Instant::now()));
}

/// Grammar returned without calling the network — disabled, under `MIN_WORDS`, or
/// pre-empted by a snippet. Pins the stage to zero at the moment the decision was
/// made, so the stages still sum to the total.
pub fn grammar_skipped() {
    with(|m| {
        let now = Instant::now();
        m.grammar_sent = Some(now);
        m.grammar_done = Some(now);
    });
}

/// `insert::type_text` has returned. Prints the report and appends a CSV row.
pub fn inserted(words: usize, cleaned: bool) {
    let end = Instant::now();

    let Ok(mut slot) = MARKS.lock() else { return };
    let Some(mut marks) = slot.take() else { return };
    marks.words = words;
    marks.cleaned = cleaned;

    let ms = |from: Instant, to: Instant| to.saturating_duration_since(from).as_secs_f64() * 1000.0;

    // Any mark that never happened collapses to the previous one, so a stage that
    // did not run reads as 0 rather than shifting the others.
    let t0 = marks.t0;
    let wav = marks.wav_ready.unwrap_or(t0);
    let w_sent = marks.whisper_sent.unwrap_or(wav);
    let w_done = marks.whisper_done.unwrap_or(w_sent);
    let g_sent = marks.grammar_sent.unwrap_or(w_done);
    let g_done = marks.grammar_done.unwrap_or(g_sent);

    // The stage boundaries are drawn so that the five rows sum exactly to the
    // total: the wav→send handoff is folded into audio processing, since it is the
    // same synchronous stretch of work.
    let audio = ms(t0, w_sent);
    let whisper = ms(w_sent, w_done);
    let gap = ms(w_done, g_sent);
    let grammar = ms(g_sent, g_done);
    let insertion = ms(g_done, end);
    let total = ms(t0, end);

    eprintln!(
        "\n[latency] {:.1}s audio, {} words, grammar {}\n\
         Audio processing       {:.0} ms\n\
         Groq Whisper           {:.0} ms\n\
         Gap                    {:.0} ms\n\
         Grammar                {:.0} ms\n\
         Insertion              {:.0} ms\n\
         -----------------------------\n\
         Total                  {:.0} ms\n",
        marks.audio_secs,
        marks.words,
        if marks.cleaned { "applied" } else { "skipped/rejected" },
        audio,
        whisper,
        gap,
        grammar,
        insertion,
        total,
    );

    append_row(&marks, audio, whisper, gap, grammar, insertion, total);
}

/// One row per dictation, so the run can be averaged afterwards.
fn append_row(
    marks: &Marks,
    audio: f64,
    whisper: f64,
    gap: f64,
    grammar: f64,
    insertion: f64,
    total: f64,
) {
    let fresh = !std::path::Path::new(LOG).exists();
    let Ok(mut file) = std::fs::OpenOptions::new().create(true).append(true).open(LOG) else {
        return;
    };

    if fresh {
        let _ = writeln!(
            file,
            "audio_secs,words,cleaned,audio_ms,whisper_ms,gap_ms,grammar_ms,insertion_ms,total_ms"
        );
    }

    let _ = writeln!(
        file,
        "{:.2},{},{},{:.0},{:.0},{:.0},{:.0},{:.0},{:.0}",
        marks.audio_secs, marks.words, marks.cleaned, audio, whisper, gap, grammar, insertion, total
    );
}
