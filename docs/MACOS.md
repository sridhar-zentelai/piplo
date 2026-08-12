# macOS Support

Piplo is Windows-first. This is the plan for making it run on macOS without
turning the codebase into a pile of `cfg!` branches.

The whole port is **four seams**. Everything else already compiles anywhere.

## Status

| Seam | State |
| ---- | ----- |
| `platform::Rect` + the `RECT` compile blocker | **Done** |
| `platform::work_area` — `NSScreen.visibleFrame` | **Done**, flip unit-tested |
| `platform::cursor_position` / `mouse_down` / `escape_down` / `modifiers_held` | **Done** |
| `insert.rs` — `CGEvent` typing, chunking unit-tested | **Done** |
| `insert::Insert` outcome + clipboard rescue in `session.rs` | **Done** — also fixes the Windows UIPI case |
| `NSMicrophoneUsageDescription`, entitlements, bundle config | **Done** |
| CI compile loop + smoke screenshot | **Done** |
| `platform::no_activate` — the non-activating panel | **Done**, needs a real Mac to confirm. Written inline with `objc2` rather than via `tauri-nspanel`, which is not on crates.io. |
| `macOSPrivateApi` for real transparency | **Done** — caught by the smoke test's stderr |
| Accessibility row in Settings | Not started |
| Per-platform shortcut default and labels | Not started |
| Template tray icon | Not started |
| `INSTALL.md` | Not started |

---

## What is already portable

| Module | Why it is fine |
| ------ | -------------- |
| `session.rs` | Pure state machine. No OS calls. |
| `audio.rs` | `cpal` speaks CoreAudio on macOS. Same default-device API. |
| `groq.rs`, `grammar.rs` | `reqwest` with `native-tls` links Secure Transport instead of Schannel. No change. |
| `history.rs`, `settings.rs`, `credentials.rs` | All go through `app.path().app_config_dir()`, which resolves to `~/Library/Application Support/com.codea.piplo`. |
| `tray.rs` | Tauri's tray works. Needs one cosmetic change (template icon). |
| `shortcut.rs` | The plugin binds on macOS via Carbon `RegisterEventHotKey`, and it does report key **up** — hold-to-talk works. Needs a policy change, not a mechanism change. |
| The entire frontend | Webview only. `shortcuts.ts` needs new labels, nothing else. |

---

## It does not compile on macOS today

`src-tauri/src/widget.rs:211`

```rust
#[cfg(not(windows))]
pub fn work_area(_window: &WebviewWindow) -> Option<RECT> { None }
```

`RECT` is imported inside `#[cfg(windows)]`, so the non-Windows stub names a
type that does not exist. `clamp()` at line 168 takes a `RECT` too, and it is
cross-platform code.

**Fix first, before anything else:** put a plain rectangle in `platform.rs` and
use it everywhere above the seam.

```rust
// platform.rs — the shape the pipeline is allowed to know about.
#[derive(Clone, Copy)]
pub struct Rect {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}
```

`widget.rs` and `menu.rs` then work in `platform::Rect`, and only the two
`work_area` implementations know about `RECT` / `NSRect`.

---

## The four seams

Nothing outside these gets a `cfg`. If a `cfg!(target_os)` appears in
`session.rs`, the seam is in the wrong place.

### 1. `platform.rs` — the widget must never take focus

This is the load-bearing one. On Windows it is `WS_EX_NOACTIVATE`. macOS has no
equivalent flag on `NSWindow`: clicking any ordinary window activates the app,
which would steal focus from the app we are about to type into.

The only mechanism that does this on macOS is an **`NSPanel` with
`NSWindowStyleMaskNonactivatingPanel`**. Tauri creates an `NSWindow`, so the
window has to be converted after creation.

Use [`tauri-nspanel`](https://github.com/ahkohd/tauri-nspanel) — one dependency,
one sentence: *it converts a Tauri window into a non-activating NSPanel, which
is the only way on macOS to click a floating window without activating the app.*
This is the same trick Raycast-style Tauri apps use.

```rust
#[cfg(target_os = "macos")]
pub fn no_activate(window: &WebviewWindow) {
    use tauri_nspanel::{WebviewWindowExt, panel_delegate};
    use objc2_app_kit::{NSWindowCollectionBehavior, NSWindowStyleMask};

    let Ok(panel) = window.to_panel() else {
        eprintln!("piplo: could not convert {} to a panel", window.label());
        return;
    };

    // NonactivatingPanel is the whole point; the rest keeps the chip visible
    // over full-screen apps and on every Space, which is where a dictation
    // widget has to live.
    panel.set_style_mask(NSWindowStyleMask::NonactivatingPanel.0 as i32);
    panel.set_level(NSFloatingWindowLevel);
    panel.set_collection_behaviour(
        NSWindowCollectionBehavior::CanJoinAllSpaces
            | NSWindowCollectionBehavior::Stationary
            | NSWindowCollectionBehavior::FullScreenAuxiliary,
    );
}
```

`hide_from_alt_tab` becomes a no-op on macOS — a panel is already out of
Cmd+Tab, because Cmd+Tab lists applications, not windows.

**Decision to make:** whether the app is `NSApplicationActivationPolicy::Regular`
(Dock icon, like now) or `Accessory` (tray only, no Dock icon). Piplo opens
`home` on launch, so `Regular` is the honest match. Revisit only if the Dock
icon feels wrong.

### 2. `insert.rs` — typing into the focused app

`SendInput` → `CGEvent` with `CGEventKeyboardSetUnicodeString`.

```rust
#[cfg(target_os = "macos")]
pub fn type_text(text: &str) {
    use core_graphics::event::{CGEvent, CGEventTapLocation};
    use core_graphics::event_source::{CGEventSource, CGEventSourceStateID};

    if text.is_empty() { return; }
    wait_for_modifiers();

    let Ok(source) = CGEventSource::new(CGEventSourceStateID::HIDSystemState) else {
        eprintln!("piplo: no CGEventSource — is Accessibility granted?");
        return;
    };

    for chunk in chunks(text) {
        // Both down and up, or apps that watch key-up (Electron, Slack) drop it.
        for down in [true, false] {
            let Ok(event) = CGEvent::new_keyboard_event(source.clone(), 0, down) else {
                continue;
            };
            event.set_string(chunk);
            event.post(CGEventTapLocation::HID);
        }
        std::thread::sleep(BATCH_GAP);
    }
}
```

Three details that are not optional:

- **Chunk at ~20 UTF-16 units.** `CGEventKeyboardSetUnicodeString` silently
  truncates past roughly that. Chunk on `char` boundaries so surrogate pairs
  never split. The existing `BATCH = 64` is a Windows number and does not carry
  over.
- **Newlines.** The Unicode string path does not deliver `\n` any more reliably
  than `SendInput` does. Post virtual keycode `36` (Return) down/up, the exact
  mirror of the `VK_RETURN` branch that already exists.
- **`wait_for_modifiers` still matters.** `Cmd` held while text is posted turns
  every character into a menu shortcut — `Cmd+S`, `Cmd+W`, `Cmd+Q` firing inside
  the user's app. Same data-loss failure, same fix. Read the modifiers with
  `NSEvent::modifierFlags()`, a class method that needs no permission, and keep
  the same 1 s ceiling and "type anyway" fallback.

### 3. `widget.rs::work_area` — where the chip is allowed to sit

`GetMonitorInfoW().rcWork` → `NSScreen.visibleFrame`, which excludes the menu bar
and the Dock. Tauri's own `current_monitor()` is not a substitute: it returns the
full frame, so the chip would sit under the Dock.

The coordinate flip is the part that gets written wrong. AppKit's origin is the
**bottom-left of the primary screen**; Tauri's `PhysicalPosition` is top-left.

```rust
let primary_height = NSScreen::screens(mtm).firstObject()?.frame().size.height;
let visible = screen.visibleFrame();
let scale = screen.backingScaleFactor();

Rect {
    left:   (visible.origin.x * scale) as i32,
    top:    ((primary_height - visible.origin.y - visible.size.height) * scale) as i32,
    right:  ((visible.origin.x + visible.size.width) * scale) as i32,
    bottom: ((primary_height - visible.origin.y) * scale) as i32,
}
```

Pick the screen containing the window, not the main one — the widget follows the
user across displays.

### 4. `menu.rs` — the right-click menu

Two pieces:

- **`cursor_pos`**: `GetCursorPos` → `NSEvent::mouseLocation()`, flipped the same
  way as above.
- **`watch_for_dismissal`**: the loop polls `GetAsyncKeyState` because a
  no-focus window gets no blur and no keys. On macOS, outside clicks come from
  `NSEvent::pressedMouseButtons()` — a class method, no permission needed.

  Escape is the awkward one: reading a global key press needs the Accessibility
  permission, which Piplo already requires for typing. So Escape-to-dismiss
  works when Accessibility is granted and degrades to click-to-dismiss when it
  is not. That is acceptable; the menu is never the thing that loses a
  dictation.

---

## Permissions — the one genuinely new concept

Windows asks for nothing. macOS asks for two things, at different times, from
different panes.

| Permission | Needed for | Check | Fails as |
| ---------- | ---------- | ----- | -------- |
| **Microphone** | `cpal` opening the input device | prompted automatically on first use | `Error { message }` on the pill |
| **Accessibility** | posting `CGEvent`s, i.e. typing | `CGPreflightPostEventAccess()` | The dictation is transcribed and **not typed** |

The second one is a trap. The pipeline succeeds — audio captured, Whisper
called, grammar applied — and then the text goes nowhere and the user has no
idea why.

## Solving it

The root cause is not macOS. It is that **`insert::type_text` returns `()`**, so
the pipeline cannot tell the difference between typing and not typing.
`session.rs:235` infers success from `spawn_blocking(...).await.is_ok()`, which
only reports whether the task panicked. `entry.inserted` in `history.jsonl` is
already unreliable on Windows for the same reason: when `SendInput` delivers 0
of N events, `insert.rs:130` prints a line and returns normally.

Four layers, outermost first. Layers 2 and 3 are cross-platform and fix a live
Windows bug, so they land before any macOS code.

### Layer 1 — do not get into the state

`CGPreflightPostEventAccess()` returns a bool without prompting.

- Call it in `setup()` and log it, next to the existing "no Groq API key" line.
- A Settings row: **Accessibility — Piplo needs permission to type into other
  apps**, with the status and a button.
- The button calls `CGRequestPostEventAccess()`. macOS shows that prompt **once
  per binary, ever** — after the first refusal it silently returns false. So if
  preflight is false and we have already asked, the button instead opens
  `x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility`.
- Re-check when `home` gains focus. Granting happens in System Settings, in
  another app, and macOS sends no notification — without the re-check the row
  stays red after the user has fixed it, which reads as the app being broken.

Deliberately **not** checked in `session::start`. Blocking a dictation on a
permission is exactly the "waiting on a dialog" the app is meant to avoid, and
the recording is still worth having with the clipboard fallback below.

### Layer 2 — make the failure impossible to swallow

Give the insert an outcome the compiler forces the caller to handle.

```rust
// insert.rs
#[must_use]
pub enum Insert {
    Typed,
    /// Nothing reached the target app. The caller owes the user a fallback.
    Blocked(&'static str),
}
```

- **macOS**: `CGPreflightPostEventAccess()` is false, or `CGEventSource::new`
  fails → `Blocked("Piplo needs Accessibility permission")`.
- **Windows**: the first `flush` delivers 0 events → `Blocked("another app
  blocked the keystrokes")`. This is a real case today: a target window running
  elevated silently discards `SendInput` from a non-elevated process (UIPI),
  and so do some anti-cheat drivers. Right now that dictation is logged as
  `inserted: true` and vanishes.
- Partial delivery stays `Typed`. It is not recoverable and re-typing would
  duplicate text.

### Layer 3 — never lose the dictation

`session.rs` handles the outcome:

```rust
let outcome = tauri::async_runtime::spawn_blocking(move || insert::type_text(&typed))
    .await
    .unwrap_or(insert::Insert::Blocked("the typing task failed"));

let inserted = matches!(outcome, insert::Insert::Typed);
```

History is appended **before** the error is shown, and with the honest
`inserted` value — the entry is the last line of defence and must be written
whatever happens next.

Then, only when blocked:

```rust
use tauri_plugin_clipboard_manager::ClipboardExt;

let message = match app.clipboard().write_text(text.clone()) {
    Ok(()) => format!("{why} — copied to the clipboard"),
    Err(_)  => format!("{why} — saved to history"),
};
fail(&app, message);
```

No new dependency: `tauri-plugin-clipboard-manager` is already in `Cargo.toml`
for the copy button on a history row.

Order matters. The clipboard write is the *fallback*, so it must not run on the
happy path — Piplo's insert path deliberately never touches the clipboard
(`insert.rs:3`), and silently overwriting what the user had copied would be its
own small data loss. Clobbering it is only justified when the alternative is
losing the dictation.

`ERROR_LINGER` is 2.5 s, tuned for "transcription failed". This message asks the
user to go do something, so it needs its own longer linger — around 6 s — or a
variant of `fail` that holds until the next session starts.

### Layer 4 — do not let it rot in development

Accessibility is granted per *binary signature*. Every unsigned `cargo tauri
dev` rebuild produces a new identity, so the permission silently drops and you
spend the afternoon debugging a port that was already correct.

Sign dev builds with a stable ad-hoc identity (`codesign -s - --force`) as a
post-build step, and treat "Accessibility: false" in the startup log as the
first thing to check when typing stops working.

### What the user sees

| State | Outcome |
| ----- | ------- |
| Permission granted | Text typed. Unchanged from Windows. |
| Never asked | Settings row is amber with a **Grant** button; first dictation types nothing, pill says why, text is on the clipboard, entry in history. |
| Refused earlier | Same, but the button opens System Settings instead of prompting. |
| Granted mid-session | Next dictation types. No restart — the preflight is read per insert. |

The dictation survives all four.

---

## Shortcuts

`shortcut.rs::reject_super` refuses Cmd/Super outright, because Windows silently
swallows chords the shell has claimed. **On macOS that rule is wrong** — Cmd is
the normal modifier, and `RegisterEventHotKey` actually returns an error on a
conflict instead of pretending to succeed.

- Gate `reject_super` to `#[cfg(windows)]`.
- On macOS, refuse nothing and let the OS reject; warn on `Cmd+Space`
  (Spotlight) and `Cmd+Tab` in the recorder.
- Move the AltGr warning in `shortcuts.ts::warningFor` behind a platform check —
  macOS has no AltGr, it has Option, and the warning would just be wrong.
- **Default shortcut per platform.** `Ctrl+Space` is "select the previous input
  source" on macOS. Use `Cmd+Shift+Space` there. `DEFAULT_ACCELERATOR` becomes a
  `cfg` constant in `shortcut.rs`, mirrored in `DEFAULT_SHORTCUT` in
  `shortcuts.ts` via a platform value handed over at startup.
- Labels: render `Cmd`/`Option`/`Control` on macOS, not `Super`/`Alt`/`Ctrl`.

---

## Build and bundle

`tauri.conf.json`:

```json
"bundle": {
  "macOS": {
    "minimumSystemVersion": "11.0",
    "entitlements": "entitlements.plist"
  }
}
```

`src-tauri/Info.plist` (Tauri v2 merges it):

```xml
<key>NSMicrophoneUsageDescription</key>
<string>Piplo records your voice while you hold the dictation shortcut.</string>
```

`src-tauri/entitlements.plist`, for the hardened runtime:

```xml
<key>com.apple.security.device.audio-input</key>
<true/>
```

**Not sandboxed.** Posting synthetic events is incompatible with the App
Sandbox, so the Mac App Store is out. Distribution is a signed and notarized
`.dmg` — Accessibility is granted per signature, so an unsigned build makes
users re-grant on every update.

`Cargo.toml`:

```toml
[target.'cfg(target_os = "macos")'.dependencies]
objc2 = "0.6"
objc2-app-kit = "0.3"
objc2-foundation = "0.3"
core-graphics = "0.24"
tauri-nspanel = "2"
```

`profile.release` has `panic = "abort"` and `strip = true`, both fine on macOS.

Tray: `TrayIconBuilder::icon_as_template(true)` plus a monochrome PNG, or the
icon renders as a coloured blob that ignores the menu bar's light/dark state.

CI: a `macos-14` runner, building `--target universal-apple-darwin`.

---

## Milestones

Same rule as the main plan — each one is verifiable on its own, do not skip
ahead.

### MAC-1 — it compiles and launches

Introduce `platform::Rect`, replace every `RECT` above the seam, stub all four
seams for macOS. Windows behaviour must be byte-identical.

**Verify:** `cargo check` passes on both targets. `cargo tauri dev` on macOS
shows three windows, a tray icon, and a chip that sits in the wrong place and
steals focus. That is the expected MAC-1 result.

### MAC-2 — focus and placement

`platform.rs` panel conversion and `widget.rs::work_area`.

**Verify:** with TextEdit focused, clicking the chip leaves the caret blinking in
TextEdit. The chip sits above the Dock, follows the widget across displays, and
survives a resolution change.

### MAC-3 — typing

`insert.rs`, the Accessibility check, the Settings row, and the clipboard
fallback.

**Verify:** a full dictation lands in TextEdit including a `—`, an emoji, and a
newline. Revoke Accessibility and repeat: the pill says why, and the text is on
the clipboard.

### MAC-4 — the rest

`menu.rs` dismissal, per-platform shortcut defaults and labels, template tray
icon, bundle config, notarization.

**Verify:** the right-click menu dismisses on an outside click without eating
that click. A fresh install on a machine that has never run Piplo prompts for
both permissions and completes a dictation.
