//! Typing the text into whatever window has focus.
//!
//! The clipboard is never touched — this is synthesised Unicode input, so the
//! user's clipboard survives every dictation.

use std::time::{Duration, Instant};

#[cfg(windows)]
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYBD_EVENT_FLAGS,
    KEYEVENTF_KEYUP, KEYEVENTF_UNICODE, VIRTUAL_KEY, VK_CONTROL, VK_LWIN, VK_MENU, VK_RETURN,
    VK_RWIN, VK_SHIFT,
};

/// How long to wait for the user to let go of their modifiers.
const MODIFIER_POLL: Duration = Duration::from_millis(10);

/// A stuck key must not hang the pipeline forever.
const MODIFIER_CEILING: Duration = Duration::from_secs(1);

/// Events per `SendInput` call. Slower targets — Electron, remote desktop — drop
/// input when it arrives in one huge burst.
const BATCH: usize = 64;

const BATCH_GAP: Duration = Duration::from_millis(2);

/// Blocking. Call it from `spawn_blocking`.
#[cfg(windows)]
pub fn type_text(text: &str) {
    if text.is_empty() {
        return;
    }

    wait_for_modifiers();

    let mut batch: Vec<INPUT> = Vec::with_capacity(BATCH);

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
            flush(&mut batch);
        }
    }

    flush(&mut batch);
}

#[cfg(not(windows))]
pub fn type_text(_text: &str) {}

/// The one non-obvious correctness requirement in the app.
///
/// The global-shortcut plugin fires on the *first* key up, which is almost always
/// `Space`, leaving `Ctrl` physically held. Typing into a live `Ctrl` turns every
/// character into a shortcut — `Ctrl+S`, `Ctrl+W`, `Ctrl+Q` firing inside the
/// user's app. That is data loss, not a glitch.
#[cfg(windows)]
fn wait_for_modifiers() {
    const MODIFIERS: [VIRTUAL_KEY; 5] = [VK_CONTROL, VK_SHIFT, VK_MENU, VK_LWIN, VK_RWIN];

    let start = Instant::now();

    while start.elapsed() < MODIFIER_CEILING {
        if MODIFIERS.iter().all(|key| !is_down(*key)) {
            return;
        }

        std::thread::sleep(MODIFIER_POLL);
    }

    // Typing anyway is the lesser evil: the alternative is silently losing the
    // dictation the user just spoke.
    eprintln!("piplo: modifiers still held after 1s, typing anyway");
}

#[cfg(windows)]
fn is_down(key: VIRTUAL_KEY) -> bool {
    // The high bit is "physically down". The low bit is "pressed since the last
    // call", which would be a false positive here.
    unsafe { (GetAsyncKeyState(key.0 as i32) as u16 & 0x8000) != 0 }
}

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

#[cfg(windows)]
fn flush(batch: &mut Vec<INPUT>) {
    if batch.is_empty() {
        return;
    }

    let sent = unsafe { SendInput(batch, std::mem::size_of::<INPUT>() as i32) };

    if sent as usize != batch.len() {
        // Games and some DirectInput apps swallow synthesised input. Logged so the
        // cause is visible rather than looking like Piplo did nothing.
        eprintln!("piplo: SendInput delivered {sent} of {} events", batch.len());
    }

    batch.clear();
    std::thread::sleep(BATCH_GAP);
}
