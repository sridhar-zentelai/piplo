# Architecture

## The one decision everything follows from

**Rust owns the dictation pipeline. React only renders state.**

Two findings force this, both learned the expensive way on Saylo:

1. **`getUserMedia` needs a secure context.** The packaged app serves
   `http://tauri.localhost`, which is not one. Mic capture in the webview works
   under `tauri dev` and silently breaks in the build — the worst possible
   failure shape.
2. **WebView2's mic permission prompt is unusable here.** `wry` only registers a
   `PermissionRequested` handler for the clipboard, so a mic request falls
   through to WebView2's own modal — a native dialog inside a 96 px frameless
   transparent window.

So capture is `cpal` on a dedicated Rust thread, and the webview never asks for
a device.

The corollary: **the frontend has no logic that can break dictation.** If the
webview hangs, the pipeline still completes and the text still lands.

---

## Data flow

```
 ┌─ shortcut.rs ──── Ctrl+Space held ──┐
 ├─ widget.rs ────── mic clicked ──────┤
 └─ menu.rs ─────── "Dictate" ─────────┘
                    │
                    ▼
             ┌─────────────┐
             │  session.rs │  the only module that knows the order
             └─────────────┘
              start │        │ finish              │ cancel
                    ▼        │                     ▼
              ┌──────────┐   │              drop samples,
              │ audio.rs │   │              bump generation,
              │  cpal    │   │              no API call
              │  thread  │   │
              └────┬─────┘   │
       samples ────┤         │
  RMS → "level" ───┘         │
      (~30 Hz)               ▼
                    downmix → 16 kHz mono → WAV (hound, in memory)
                             │
                             ▼
                        ┌─────────┐
                        │ groq.rs │  whisper-large-v3-turbo
                        └────┬────┘
                             │ raw transcript
                             ▼
                       ┌───────────┐
                       │grammar.rs │  chat completion, 2 s ceiling
                       └────┬──────┘  any failure → raw text
                            │ final text
              ┌─────────────┴─────────────┐
              ▼                           ▼
        ┌───────────┐              ┌────────────┐
        │ insert.rs │              │ history.rs │
        │ SendInput │              │  append    │
        └───────────┘              └────────────┘
```

Every stage runs off the UI thread. Network calls go on
`tauri::async_runtime::spawn`; `insert.rs` is blocking and goes on
`spawn_blocking`.

---

## Rust modules

| Module | Owns | Never does |
| ------ | ---- | ---------- |
| `main.rs` | Thin entry, calls `lib::run` | Logic |
| `lib.rs` | Builder, plugins, managed state, window setup, `.env` load | Pipeline steps |
| `platform.rs` | `WS_EX_NOACTIVATE` on the widget and menu | Anything non-Windows |
| `shortcut.rs` | Register / rebind the accelerator, hold-mode, key-repeat guard | Touch audio directly |
| `widget.rs` | Show, hide, resize, position the widget window | Know about Groq |
| `menu.rs` | Position, show, hide the `menu` window; handle its commands | Own state |
| `session.rs` | The state machine, generation counter, status emission | HTTP, file I/O, keystrokes |
| `audio.rs` | cpal thread, sample buffer, RMS, downmix, WAV encode | Know why it's recording |
| `groq.rs` | Transcription request and typed errors | Retry policy |
| `grammar.rs` | Cleanup request, output validation, fail-soft | Return `Result` (returns `Option`) |
| `insert.rs` | Wait for modifiers, `SendInput` Unicode | Decide *what* to type |
| `history.rs` | Append and read `history.jsonl` | Filter or search (the UI does) |
| `settings.rs` | Load, validate, save `settings.json` | Apply settings (owners do) |
| `tray.rs` | Tray icon and its menu | Duplicate `menu.rs` logic |

**`session.rs` is the seam.** It is the only file that changes when the order of
operations changes. Nothing else knows there are two network calls.

---

## The state machine

```rust
enum Status {
    Idle,
    Recording,
    Transcribing,          // covers transcription AND grammar
    Error { message: String },
}
```

Transitions:

| From | Trigger | To |
| ---- | ------- | -- |
| `Idle` | shortcut down / mic click / menu item | `Recording` |
| `Recording` | shortcut up / ✓ | `Transcribing` |
| `Recording` | ✕ | `Idle` (samples dropped, no call) |
| `Transcribing` | text typed and logged | `Idle` |
| `Transcribing` | ✕ | `Idle` (result discarded on arrival) |
| any | failure | `Error` → `Idle` after ~2.5 s |

**`Transcribing` deliberately covers grammar too.** No "polishing" state, no
second spinner. The user should not be able to tell there are two calls.

### The generation counter

`WidgetState` holds a `u64` bumped on every start and every cancel. The value is
captured before each network call and compared after it returns; a mismatch
discards the result.

Without this, cancelling and immediately re-recording types the *previous*
take's text into the document — a bug that only shows up under exactly the
impatient usage the app invites.

The check runs after **both** awaits, transcription and grammar.

---

## Events

Two, and only two, cross the boundary into the webview.

| Event | Payload | Rate | Why separate |
| ----- | ------- | ---- | ------------ |
| `status` | tagged `Status` | on change | Drives the whole tree |
| `level` | `f32`, 0.0–1.0 | ~30 Hz | Re-rendering the tree 30×/s for a waveform would be absurd |

`level` is throttled in the cpal callback (~33 ms), not in JS. The callback fires
hundreds of times a second; throttling at the source is the only place that
actually saves work.

---

## Commands

Typed wrappers live in `src/lib/commands.ts`. Nothing calls `invoke` directly.

| Command | Returns | Used by |
| ------- | ------- | ------- |
| `start_dictation` | `()` | Mic button, menu |
| `finish_dictation` | `()` | ✓ button |
| `cancel_dictation` | `()` | ✕ button |
| `get_settings` | `Settings` | Settings page, menu |
| `set_settings` | `Result<Settings, String>` | Settings page, menu toggle |
| `get_history` | `Vec<Entry>` | Home page |
| `clear_history` | `()` | Home page |
| `open_home` | `()` | Tray, menu |
| `show_widget_menu` | `()` | Right-click on the widget |
| `quit` | never | Tray, menu |

---

## Frontend

One bundle, three roots. `main.tsx` reads the current window label and mounts
accordingly:

```
widget  → <FloatingWidget />
menu    → <WidgetMenu />
home    → <DesktopWindow />
```

This keeps the widget's JS payload as small as the shared bundle allows and
avoids three Vite entry points and three HTML files.

### Store

`appStore.ts` (Zustand) holds only what the widget needs to render:

```ts
{
  status: "idle" | "recording" | "transcribing" | "error"
  level: number          // 0..1, smoothed in Waveform, not here
  errorMessage: string | null
  hovered: boolean
}
```

History and settings are **not** in the store. They are fetched by the pages
that show them (`useHistory`, and local state in `SettingsPage`) — putting
machine state that only one window reads into a global store buys nothing.

---

## What is not in this design

- **No IPC from the webview into the pipeline mid-flight.** Once a session
  starts, the only thing the UI can do is cancel it.
- **No retries.** A failed transcription is an error the user sees and repeats;
  a failed grammar call is invisible. Neither benefits from an automatic retry
  the user is waiting through.
- **No queue.** One session at a time. Starting a second one while the first is
  in flight is prevented in `session.rs`, not debounced in the UI.
- **No database.** History is append-only JSONL, read whole. At the volume one
  person generates, a schema and migrations would be pure cost.
