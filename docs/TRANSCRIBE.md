# Transcribe

```
hold Ctrl+Space  →  capture mic  →  release  →  Groq whisper-large-v3-turbo
                                                 ↓
                    text typed into whatever app had focus
                                                 ↓
                    appended to history/history.jsonl
```

Rust owns every stage. React only renders `status` and `level`.

---

## `audio.rs`

### The thread

cpal's `Stream` is `!Send` on WASAPI, so it cannot be held in Tauri's managed
state. The stream lives on its own thread that parks until told to stop:

- `start(app) -> Recorder` — spawns a thread, opens the default input device,
  builds an input stream, keeps it alive until a stop signal arrives on an
  `mpsc` channel.
- The data callback appends to `Arc<Mutex<Vec<f32>>>` and accumulates RMS. A
  ~33 ms throttle emits `level` (`f32`, 0.0–1.0).
- `stop(self) -> (Vec<f32>, SampleRate, u16)` — signals, joins, and returns the
  samples with the format actually negotiated.

Handle all three of cpal's sample formats — `F32`, `I16`, `U16`. WASAPI usually
gives `F32`; do not assume it.

**Throttle in the callback, not in JS.** It fires hundreds of times a second.

### Downmix and resample

Whisper wants **16 kHz mono**. WASAPI shared mode hands back the device mixer
format, typically 48 kHz stereo.

1. Average channels to mono.
2. Box-average decimate to 16 kHz. 48 k → 16 k is exactly 3:1; other rates use a
   fractional box filter.

**Do not drop samples naively.** It aliases, and it measurably hurts
transcription accuracy on sibilants. The box filter is a few lines and removes
the problem.

This also cuts the upload ~3×, which is latency the user feels on every single
dictation.

### Encode

16-bit PCM WAV in memory via `hound::WavWriter` over a `Cursor<Vec<u8>>`. No
temp file — nothing to clean up, nothing left behind on a crash.

---

## `groq.rs`

```
POST https://api.groq.com/openai/v1/audio/transcriptions
Authorization: Bearer $GROQ_API_KEY
multipart/form-data:
  file             audio.wav  (audio/wav)
  model            whisper-large-v3-turbo
  temperature      0
  response_format  verbose_json
```

`verbose_json` over plain `json` because it returns `language` and `duration`,
both of which go straight into the history entry. Deserialize `text`, `language`,
`duration`; ignore the rest.

Map non-2xx into a typed error carrying Groq's own message, so the UI can say
something better than "failed":

| Status | Meaning | Shown as |
| ------ | ------- | -------- |
| 401 | Bad or missing key | "Check your API key" |
| 413 | Audio too long | "Recording too long" |
| 429 | Rate limited | "Rate limited, try again" |
| other | — | Groq's message, truncated |

Runs on `tauri::async_runtime::spawn` — Tauri's tokio runtime is already there,
so no second runtime.

### The key

`dotenvy::from_path(repo_root().join(".env"))` in `lib.rs` setup, falling back to
the process environment, read once into managed state. Missing key → log it and
let the UI show the error state. **Do not panic** — a missing key should be a
message, not a crash.

The key is never sent to the webview.

---

## `insert.rs`

### Wait for modifiers first

This is the one non-obvious correctness requirement in the whole app.

The user just released `Ctrl+Space`. The global-shortcut plugin fires on the
*first* key up — which is almost always `Space`, leaving `Ctrl` physically held.
Typing into a live `Ctrl` turns every character into a shortcut: `Ctrl+S`,
`Ctrl+W`, `Ctrl+Q` firing inside the user's app. That is data loss, not a glitch.

So: poll `GetAsyncKeyState` for `VK_CONTROL`, `VK_SHIFT`, `VK_MENU`, `VK_LWIN`,
`VK_RWIN` every 10 ms until all are up, with a **1 s ceiling** so a stuck key
cannot hang the pipeline forever.

### Then type

`SendInput` with `KEYEVENTF_UNICODE`: one key-down + key-up pair per UTF-16 code
unit, surrogate pairs sent as two units.

- `\n` cannot go through the Unicode path reliably — send `VK_RETURN` instead.
- Batch ~64 events per `SendInput` call with a 1–2 ms gap, so slower apps
  (Electron, remote desktop) don't drop input.

Blocking work — call it via `spawn_blocking`.

Focus is already correct: the widget carries `WS_EX_NOACTIVATE`, so neither
showing the pill nor clicking the mic ever took focus from the target app. See
[WIDGET.md](WIDGET.md#why-it-never-takes-focus).

### Clipboard is never touched

Simulated Unicode typing, not copy-paste. The user's clipboard survives every
dictation — verified explicitly in
[M2's checks](MVP_PLAN.md#m2--transcribe).

---

## `history.rs`

One line appended per dictation to `history/history.jsonl` at the repo root:

```json
{"id":"…","at":"2026-08-11T21:40:12Z","duration_secs":4.2,
 "model":"whisper-large-v3-turbo","language":"en","chars":128,
 "inserted":true,"text":"…","raw_text":"…","corrected":true}
```

| Field | Notes |
| ----- | ----- |
| `id` | `uuid::Uuid::new_v4()` |
| `at` | RFC 3339, UTC. `time` with `parsing` enabled so it can be read back |
| `text` | **What was actually typed** — after grammar |
| `raw_text` | What Whisper returned. Only present when it differs |
| `corrected` | Whether grammar won or the fallback did |
| `inserted` | False if typing failed but transcription succeeded |

Repo root comes from `env!("CARGO_MANIFEST_DIR")/..`, baked at compile time.
If that path doesn't exist — a build moved to another machine — fall back to
`app_data_dir()` so a packaged binary still records instead of silently dropping
data.

`OpenOptions::new().create(true).append(true)` and one `write_all` per line.
Append mode is atomic enough for single-process line writes; no locking.

Reading is whole-file, newest-first, malformed lines skipped rather than fatal —
one bad line from an interrupted write must not hide the entire history.

---

## `session.rs`

The only module that knows the order.

- **start** (shortcut hold / mic click / menu item) → `audio::start`, status
  `Recording`
- **finish** (key release / ✓) → stop capture → status `Transcribing` → encode →
  `groq::transcribe` → `grammar::run` → `insert::type_text` →
  `history::append` → status `Idle`
- **cancel** (✕) → stop capture, drop the samples, no API call, status `Idle`
- **failure** → status `Error`, auto-clear to `Idle` after ~2.5 s

### Guards

**Stale results.** A generation counter in `WidgetState`, bumped on every start
and cancel, captured before each await and compared after. Mismatch → discard.
Checked after **both** network calls. Without it, cancel-then-immediately-record
types the previous take's text.

**Accidental taps.** Skip the entire pipeline when the recording is under
~300 ms or the peak amplitude never leaves the noise floor. A brushed key should
not cost an API call.

**One at a time.** A start while a session is in flight is rejected in
`session.rs`, not debounced in the UI — the shortcut, the mic button, and the
menu are three entry points and only one place should enforce this.

---

## Latency budget

Roughly network round trip + ~0.5 s of Groq compute for a short clip, plus
whatever [grammar](GRAMMAR.md) adds (~300–800 ms, capped at 2 s).

Downsampling to 16 kHz mono is what keeps the upload small enough for this to
feel instant. It is not an optimisation to skip.

---

## Known limits

- **Unicode `SendInput` can be dropped** by games and some DirectInput-based
  apps. The fallback would be clipboard + `Ctrl+V`; the seam is `insert.rs`.
- **Groq caps audio at 25 MB** — ~13 minutes at 16 kHz mono. Longer recordings
  fail with a 413 rather than a clean warning. A duration cap is a deliberate
  gap, not an oversight.
- **`history.jsonl` contains everything you dictate.** It lives in the repo for
  convenience but is gitignored, so it is local to the clone and is lost if the
  working tree is deleted. See
  [README.md](../README.md#where-your-data-lives).
- **Default input device only.** No device picker.
