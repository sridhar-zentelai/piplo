//! Typing the text into whatever window has focus.
//!
//! The clipboard is never touched on the happy path — this is synthesised
//! Unicode input, so the user's clipboard survives every dictation. `session.rs`
//! falls back to the clipboard only when `type_text` reports `Blocked`, where the
//! alternative is losing the dictation outright.

use std::time::{Duration, Instant};

use crate::platform;

#[cfg(windows)]
use windows::Win32::UI::Input::KeyboardAndMouse::{
    SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYBD_EVENT_FLAGS, KEYEVENTF_KEYUP,
    KEYEVENTF_UNICODE, VIRTUAL_KEY, VK_BACK, VK_RETURN,
};

/// A ceiling on `erase`. Undoing a word fix rubs out what Piplo just typed; a
/// count larger than any plausible dictation means the state is wrong, and
/// hammering backspace into someone's document is the worst possible way to find
/// out.
const MAX_ERASE: usize = 5000;

/// How long to wait for the user to let go of their modifiers.
const MODIFIER_POLL: Duration = Duration::from_millis(10);

/// A stuck key must not hang the pipeline forever.
const MODIFIER_CEILING: Duration = Duration::from_secs(1);

/// What actually happened. `#[must_use]` because the whole point is that the
/// caller cannot go back to inferring success from "the task did not panic".
#[must_use]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Insert {
    Typed,
    /// Nothing reached the target application. The caller owes the user a
    /// fallback — the text is still in hand at this point, and must not be
    /// dropped.
    Blocked(&'static str),
}

/// Blocking. Call it from `spawn_blocking`.
#[cfg(windows)]
pub fn type_text(text: &str) -> Insert {
    if text.is_empty() {
        return Insert::Typed;
    }

    wait_for_modifiers();

    let mut batch: Vec<INPUT> = Vec::with_capacity(BATCH);
    let mut delivered = false;

    for ch in text.chars() {
        if ch == '\n' {
            // The Unicode path does not deliver newlines reliably.
            batch.push(virtual_key(VK_RETURN, KEYBD_EVENT_FLAGS(0)));
            batch.push(virtual_key(VK_RETURN, KEYEVENTF_KEYUP));
        } else {
            let mut units = [0u16; 2];

            // Surrogate pairs go as two units, which is what SendInput expects.
            for unit in ch.encode_utf16(&mut units) {
                batch.push(unicode_unit(*unit, KEYEVENTF_UNICODE));
                batch.push(unicode_unit(*unit, KEYEVENTF_UNICODE | KEYEVENTF_KEYUP));
            }
        }

        if batch.len() >= BATCH {
            delivered |= flush(&mut batch);
        }
    }

    delivered |= flush(&mut batch);

    if delivered {
        Insert::Typed
    } else {
        // A target window running elevated silently discards SendInput from a
        // non-elevated process (UIPI), and so do some anti-cheat drivers. Before
        // this returned a value, that dictation was logged as inserted and lost.
        Insert::Blocked("another app blocked the keystrokes")
    }
}

/// macOS types by attaching the text to a synthetic key event, rather than
/// sending one event per character as Windows does.
#[cfg(target_os = "macos")]
pub fn type_text(text: &str) -> Insert {
    use objc2_core_graphics::{
        CGEvent, CGEventSource, CGEventSourceStateID, CGEventTapLocation,
        CGPreflightPostEventAccess,
    };

    /// Return, in Apple's positional keycodes. The Unicode string path does not
    /// deliver newlines any more reliably than `SendInput` does.
    const RETURN: u16 = 36;

    if text.is_empty() {
        return Insert::Typed;
    }

    // Checked per insert rather than cached at startup, so ticking the box in
    // System Settings applies to the next dictation instead of the next launch.
    if !CGPreflightPostEventAccess() {
        return Insert::Blocked("Piplo needs Accessibility permission to type");
    }

    wait_for_modifiers();

    let Some(source) = CGEventSource::new(CGEventSourceStateID::HIDSystemState) else {
        eprintln!("piplo: could not create a CGEventSource");
        return Insert::Blocked("macOS refused to create an input source");
    };

    for piece in pieces(text) {
        match piece {
            Piece::Newline => {
                for down in [true, false] {
                    let Some(event) = CGEvent::new_keyboard_event(Some(&*source), RETURN, down)
                    else {
                        continue;
                    };
                    CGEvent::post(CGEventTapLocation::HIDEventTap, Some(&*event));
                }
            }
            Piece::Text(chunk) => {
                let units: Vec<u16> = chunk.encode_utf16().collect();

                // Down and up both carry the string. Apps that act on key-up —
                // Electron ones especially — drop text that only ever went down.
                for down in [true, false] {
                    let Some(event) = CGEvent::new_keyboard_event(Some(&*source), 0, down) else {
                        continue;
                    };

                    unsafe {
                        CGEvent::keyboard_set_unicode_string(
                            Some(&*event),
                            // `UniCharCount` is `c_ulong`, not `usize`, and Rust
                            // will not coerce between them.
                            units.len() as _,
                            units.as_ptr(),
                        );
                    }

                    CGEvent::post(CGEventTapLocation::HIDEventTap, Some(&*event));
                }
            }
        }

        std::thread::sleep(BATCH_GAP);
    }

    // Posting is fire-and-forget: CoreGraphics reports nothing back. The
    // preflight above is the real check, which is why it is not optional.
    Insert::Typed
}

#[cfg(not(any(windows, target_os = "macos")))]
pub fn type_text(_text: &str) -> Insert {
    Insert::Blocked("typing is not implemented on this platform")
}

/// Rub out `count` characters, then type `text` in their place.
///
/// Blocking, like `type_text`, and it assumes the caret has not moved since Piplo
/// typed — which is why it is only ever offered for the most recent dictation and
/// is described to the user as an undo of that.
pub fn replace_typed(count: usize, text: &str) -> Insert {
    if count > MAX_ERASE {
        return Insert::Blocked("too much text to undo");
    }

    match erase(count) {
        Insert::Typed => type_text(text),
        blocked => blocked,
    }
}

#[cfg(windows)]
fn erase(count: usize) -> Insert {
    if count == 0 {
        return Insert::Typed;
    }

    wait_for_modifiers();

    let mut batch: Vec<INPUT> = Vec::with_capacity(BATCH);
    let mut delivered = false;

    for _ in 0..count {
        batch.push(virtual_key(VK_BACK, KEYBD_EVENT_FLAGS(0)));
        batch.push(virtual_key(VK_BACK, KEYEVENTF_KEYUP));

        if batch.len() >= BATCH {
            delivered |= flush(&mut batch);
        }
    }

    delivered |= flush(&mut batch);

    if delivered {
        Insert::Typed
    } else {
        Insert::Blocked("another app blocked the keystrokes")
    }
}

#[cfg(target_os = "macos")]
fn erase(count: usize) -> Insert {
    use objc2_core_graphics::{
        CGEvent, CGEventSource, CGEventSourceStateID, CGEventTapLocation, CGPreflightPostEventAccess,
    };

    /// Delete, in Apple's positional keycodes.
    const DELETE: u16 = 51;

    if count == 0 {
        return Insert::Typed;
    }

    if !CGPreflightPostEventAccess() {
        return Insert::Blocked("Piplo needs Accessibility permission to type");
    }

    wait_for_modifiers();

    let Some(source) = CGEventSource::new(CGEventSourceStateID::HIDSystemState) else {
        return Insert::Blocked("macOS refused to create an input source");
    };

    for _ in 0..count {
        for down in [true, false] {
            if let Some(event) = CGEvent::new_keyboard_event(Some(&*source), DELETE, down) {
                CGEvent::post(CGEventTapLocation::HIDEventTap, Some(&*event));
            }
        }
    }

    Insert::Typed
}

#[cfg(not(any(windows, target_os = "macos")))]
fn erase(_count: usize) -> Insert {
    Insert::Blocked("typing is not implemented on this platform")
}

/// The one non-obvious correctness requirement in the app.
///
/// The global-shortcut plugin fires on the *first* key up, which is almost always
/// the non-modifier key, leaving `Ctrl` — or `Cmd` on macOS — physically held.
/// Typing into a live modifier turns every character into a shortcut: `Ctrl+S`,
/// `Ctrl+W`, `Ctrl+Q` firing inside the user's app. That is data loss, not a
/// glitch.
#[cfg(any(windows, target_os = "macos"))]
fn wait_for_modifiers() {
    let start = Instant::now();

    while start.elapsed() < MODIFIER_CEILING {
        if !platform::modifiers_held() {
            return;
        }

        std::thread::sleep(MODIFIER_POLL);
    }

    // Typing anyway is the lesser evil: the alternative is silently losing the
    // dictation the user just spoke.
    eprintln!("piplo: modifiers still held after 1s, typing anyway");
}

/// One unit of work for the macOS path.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
#[derive(Debug, PartialEq, Eq)]
enum Piece<'a> {
    Text(&'a str),
    Newline,
}

/// Split text into postable pieces: newlines on their own, everything else in
/// runs short enough for `CGEventKeyboardSetUnicodeString` to deliver whole.
///
/// Compiled and tested on every platform on purpose. The two ways this can be
/// subtly wrong — truncating a long paragraph, or splitting an emoji down the
/// middle of its surrogate pair — both produce text that looks almost right, and
/// neither is something a compiler or a screenshot would catch.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn pieces(text: &str) -> Vec<Piece<'_>> {
    /// UTF-16 units per event. Apple documents no limit, but the call becomes
    /// unreliable past roughly 20 in practice, so this stays comfortably under.
    const MAX_UNITS: usize = 16;

    let mut out = Vec::new();

    for (index, line) in text.split('\n').enumerate() {
        if index > 0 {
            out.push(Piece::Newline);
        }

        let mut start = 0;
        let mut units = 0;

        for (offset, ch) in line.char_indices() {
            // Splitting on a char boundary is what keeps a surrogate pair whole:
            // an astral character contributes 2 units and is never divided.
            if units + ch.len_utf16() > MAX_UNITS {
                out.push(Piece::Text(&line[start..offset]));
                start = offset;
                units = 0;
            }

            units += ch.len_utf16();
        }

        // A trailing empty run would post an empty event; two adjacent newlines
        // legitimately produce one, and it is skipped here rather than by the
        // caller.
        if start < line.len() {
            out.push(Piece::Text(&line[start..]));
        }
    }

    out
}

/// Events per `SendInput` call. Slower targets — Electron, remote desktop — drop
/// input when it arrives in one huge burst.
#[cfg(windows)]
const BATCH: usize = 64;

const BATCH_GAP: Duration = Duration::from_millis(2);

#[cfg(windows)]
fn unicode_unit(unit: u16, flags: KEYBD_EVENT_FLAGS) -> INPUT {
    input(VIRTUAL_KEY(0), unit, flags)
}

#[cfg(windows)]
fn virtual_key(key: VIRTUAL_KEY, flags: KEYBD_EVENT_FLAGS) -> INPUT {
    input(key, 0, flags)
}

#[cfg(windows)]
fn input(key: VIRTUAL_KEY, scan: u16, flags: KEYBD_EVENT_FLAGS) -> INPUT {
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: key,
                wScan: scan,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: 0,
            },
        },
    }
}

/// `true` when at least one event was accepted.
#[cfg(windows)]
fn flush(batch: &mut Vec<INPUT>) -> bool {
    if batch.is_empty() {
        return false;
    }

    let sent = unsafe { SendInput(batch, std::mem::size_of::<INPUT>() as i32) } as usize;

    if sent != batch.len() {
        eprintln!("piplo: SendInput delivered {sent} of {} events", batch.len());
    }

    batch.clear();
    std::thread::sleep(BATCH_GAP);

    // Partial delivery still counts as typed. It is not recoverable, and
    // re-typing from the clipboard would duplicate whatever did arrive.
    sent > 0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text_of(pieces: &[Piece<'_>]) -> String {
        pieces
            .iter()
            .map(|piece| match piece {
                Piece::Text(text) => *text,
                Piece::Newline => "\n",
            })
            .collect()
    }

    /// The property that matters most: whatever the chunking does, reassembling
    /// the pieces must give back exactly the input.
    #[test]
    fn pieces_reassemble_to_the_input() {
        for input in [
            "hello",
            "",
            "a\nb",
            "\n",
            "\n\n\n",
            "trailing\n",
            "\nleading",
            "a much longer sentence that certainly exceeds one chunk of sixteen units",
            "emoji 🎙️ and an em—dash, plus accents: éàü",
            "🎙️🎙️🎙️🎙️🎙️🎙️🎙️🎙️🎙️🎙️",
        ] {
            assert_eq!(text_of(&pieces(input)), input, "round trip failed: {input:?}");
        }
    }

    #[test]
    fn short_text_is_one_piece() {
        assert_eq!(pieces("hello"), vec![Piece::Text("hello")]);
    }

    #[test]
    fn empty_text_has_no_pieces() {
        assert_eq!(pieces(""), vec![]);
    }

    /// No piece may exceed the limit, or macOS truncates it silently.
    #[test]
    fn no_piece_exceeds_the_unit_limit() {
        let long = "the quick brown fox jumps over the lazy dog, repeatedly and at length";

        for piece in pieces(long) {
            if let Piece::Text(text) = piece {
                let units = text.encode_utf16().count();
                assert!(units <= 16, "{units} units in {text:?}");
            }
        }
    }

    /// An emoji is two UTF-16 units. Splitting one leaves a lone surrogate, which
    /// renders as a replacement character — so a chunk boundary must never land
    /// inside one.
    #[test]
    fn surrogate_pairs_are_never_split() {
        // Nine emoji is 18 units: guaranteed to cross a 16-unit boundary.
        let emoji = "😀".repeat(9);

        for piece in pieces(&emoji) {
            if let Piece::Text(text) = piece {
                assert!(text.chars().all(|ch| ch == '😀'), "split a pair: {text:?}");
                assert!(text.encode_utf16().count() % 2 == 0);
            }
        }
    }

    #[test]
    fn newlines_become_their_own_piece() {
        assert_eq!(
            pieces("a\nb"),
            vec![Piece::Text("a"), Piece::Newline, Piece::Text("b")]
        );
    }

    /// A blank line is two newlines with nothing between them, and must not
    /// produce an empty text piece — that would post an event carrying no text.
    #[test]
    fn blank_lines_emit_no_empty_text() {
        assert_eq!(pieces("\n\n"), vec![Piece::Newline, Piece::Newline]);

        assert_eq!(
            pieces("a\n\nb"),
            vec![
                Piece::Text("a"),
                Piece::Newline,
                Piece::Newline,
                Piece::Text("b"),
            ]
        );

        for piece in pieces("a\n\n\nb") {
            assert_ne!(piece, Piece::Text(""));
        }
    }

    /// A realistic dictation: several sentences, punctuation the grammar model
    /// likes to add, and no newlines at all.
    #[test]
    fn a_paragraph_chunks_without_loss() {
        let paragraph = "I think we should ship the macOS build this week. \
                         It needs the permission flow first — otherwise nobody \
                         will know why it isn't typing.";

        let pieces = pieces(paragraph);

        assert!(pieces.len() > 1, "expected chunking");
        assert_eq!(text_of(&pieces), paragraph);
    }
}
