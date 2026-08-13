# CLAUDE.md

# Piplo

A lightweight desktop dictation assistant. Hold a shortcut, speak, and clean
text appears in whatever app you were already using.

## Vision

Piplo lives in the system tray behind a small floating widget.

Hold `Ctrl+Space`, speak, release. The audio goes to Groq Whisper, the
transcript is cleaned up by a Groq chat model, and the result is typed into the
focused application. Every dictation is logged so you can find it again.

Native, instant, distraction-free. The user should never wait on a dialog, and
should never lose a dictation to a failure.

---

# Scope

Piplo has **five** features and nothing else:

| # | Feature | What it is |
| - | ------- | ---------- |
| 1 | **Transcribe** | Global shortcut → mic capture → Groq Whisper → typed into the focused app |
| 2 | **Grammar** | The transcript is cleaned up automatically before it is typed. No button, no picker |
| 3 | **Home + history** | A desktop window listing past dictations, newest first, with copy |
| 4 | **Settings** | Three settings: shortcut, grammar on/off, widget visible |
| 5 | **Snippets** | Say a saved trigger on its own and Piplo types the canned text instead |

Plus the surface they live on:

- **Floating widget** — a chip that morphs into a recording pill
- **Right-click menu** on the widget — dictate, grammar toggle, open Piplo, quit
- **Tray icon** — open, quit

## DO NOT implement

Not "later" — do not write code for these at all:

- Local Whisper / whisper.cpp
- Multiple AI providers or a provider picker
- Translation
- Prompt templates, rewrite styles, per-app tone
- Keyboard-triggered text expansion — [snippets](docs/SNIPPETS.md) are spoken
  only, and never watch what you type
- Voice commands
- Clipboard history
- Authentication, billing, accounts
- Cloud sync
- Auto-update
- Onboarding flows
- Analytics
- Light theme
- Linux support

**Before adding anything, ask: is this one of the five features?** If not, it
does not get built. Piplo is deliberately smaller than it could be.

---

# Tech Stack

Identical to Saylo — no new choices to make.

- **Tauri v2** (Rust) — windows, tray, global shortcut, all native work
- **React 19** + **TypeScript** + **Vite** — the webview
- **Tailwind CSS v4** (`@tailwindcss/vite`) + **shadcn/ui** + **Radix**
- **Framer Motion** — the widget morph
- **Zustand** — frontend state
- **Bun** — package manager and script runner

Rust crates: `cpal` (capture), `hound` (WAV), `reqwest` (HTTP, `native-tls`),
`dotenvy`, `serde`, `time`, `uuid`, plus the `global-shortcut` and
`window-state` Tauri plugins.

Per-platform, behind `[target.'cfg(...)'.dependencies]`:

| Target | Crates | For |
| ------ | ------ | --- |
| Windows | `windows` | `SendInput`, `WS_EX_NOACTIVATE`, `GetMonitorInfoW` |
| macOS | `objc2`, `objc2-app-kit`, `objc2-foundation`, `core-graphics` | `CGEvent` typing, non-activating panel, `visibleFrame` |

**Do not add a dependency without a reason that fits in one sentence.**

---

# AI Provider

**Groq, for both calls.** One API key, `GROQ_API_KEY`, read only in Rust and
never exposed to the webview.

| Job | Endpoint | Model |
| --- | -------- | ----- |
| Transcription | `POST /openai/v1/audio/transcriptions` | `whisper-large-v3-turbo` |
| Grammar | `POST /openai/v1/chat/completions` | `qwen/qwen3.6-27b` |

Base URL `https://api.groq.com`. The grammar model is overridable with
`PIPLO_GRAMMAR_MODEL` so a wrong or retired id can be fixed without a rebuild.

---

# Architecture

**Rust owns the pipeline. React only renders state.**

Audio capture cannot live in the webview: `getUserMedia` needs a secure context
and the packaged app serves `http://tauri.localhost`, so it would work in dev
and break in the build. WebView2's own mic permission prompt is also unusable
inside a small frameless transparent window. So capture is `cpal` in Rust.

```
shortcut / mic click / menu item
              │
              ▼
         session.rs   ← the only module that knows the order
              │
   ┌──────────┴──────────┐
   ▼                     ▼
audio.rs             (release)
 cpal thread             │
 ├─ samples ─────────────┤
 └─ RMS → "level" event  │
                         ▼
              16 kHz mono WAV (hound)
                         │
                         ▼
                     groq.rs        (transcribe)
                         │
                         ▼
                   snippets.rs      (trigger? → content, skip grammar)
                         │
                         ▼
                    grammar.rs      (clean up, fails soft)
                         │
                         ▼
                   snippets.rs      (trigger the cleanup revealed?)
                         │
              ┌──────────┴──────────┐
              ▼                     ▼
          insert.rs             history.rs
      wait for modifiers,      append one JSONL
      SendInput Unicode        line per dictation
```

The frontend hears exactly two events: `status` (a tagged state) and `level` (a
float, ~30 Hz, kept separate so the waveform does not re-render the tree).

```rust
enum Status { Idle, Recording, Transcribing, Error { message: String } }
```

See [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) for the full module map.

---

# The Governing Rule

**A failure must never cost the user their dictation.**

Grammar is a second network call in the hot path. Every way it can fail —
timeout, 404 on the model id, rate limit, no network, a garbled or chatty
response — falls back to typing the raw transcript. The feature is invisible
when it works and invisible when it doesn't.

This is what makes it safe to have at all.

---

# Windows

Three windows, declared in `tauri.conf.json`.

| Label | Size | Purpose |
| ----- | ---- | ------- |
| `widget` | 40×40, grows to the pill | Frameless, transparent, always on top, `skipTaskbar`, **never takes focus** |
| `menu` | ~212×180 | The right-click menu. Transparent, hidden until summoned |
| `home` | 880×600 | Normal decorated window: history + settings. Opens on launch |

The widget must never activate. On Windows that is `WS_EX_NOACTIVATE`; on macOS
it is an `NSPanel` with `nonactivatingPanel` — both set in `platform.rs`. This
is load bearing: it is why showing the pill or clicking the mic never steals
focus from the app the text is about to be typed into.

---

# Platforms

Windows and macOS. Not Linux.

Everything platform-specific lives behind four seams, and **only** these four.
`session.rs` and everything above it are pure Rust that compiles anywhere.

| Seam | Windows | macOS |
| ---- | ------- | ----- |
| `platform.rs` — no-focus windows | `WS_EX_NOACTIVATE` + `WS_EX_TOOLWINDOW` | `NSPanel` + `nonactivatingPanel`, `NSWindowCollectionBehavior` |
| `insert.rs` — typing | `SendInput` (UTF-16 units) | `CGEvent` + `keyboardSetUnicodeString` |
| `widget.rs::work_area` — where the chip sits | `GetMonitorInfoW().rcWork` | `NSScreen.visibleFrame` (flipped to top-left origin) |
| `menu.rs::cursor_pos` — right-click origin | `GetCursorPos` | `NSEvent.mouseLocation` (flipped) |

Rules for the seams:

- Each seam is `#[cfg(target_os = "…")]` **with a stub for every other target**,
  and the stub must be typed in terms the whole tree can name — no `RECT` in a
  cross-platform signature. Use `Rect { left, top, right, bottom }` in
  `platform.rs`.
- The pipeline never branches on the OS. If a `cfg!` appears in `session.rs`,
  the seam is in the wrong place.
- macOS needs permissions Windows does not: Accessibility (to type) and
  Microphone (to record). A denied permission is a message on the pill and a
  link to the right Settings pane, never a crash and never a lost dictation.

`Cmd` is the macOS chord modifier, so the Windows-key refusal in `shortcut.rs`
must not apply there — `Cmd+Space` is Spotlight, but `Cmd+Shift+Space` is
ordinary. Refuse per platform, not globally.

Port plan and milestones: [docs/MACOS.md](docs/MACOS.md).

---

# UI Principles

- Minimal, modern, rounded
- Transparent background, soft blur
- Smooth but quick animations
- Premium feel
- No unnecessary buttons

Dark mode only. There is no light theme and no theme switch.

Inspired by Whisper Flow, Raycast, Spotlight, Linear, Arc.

---

# Colors

```
Background      rgba(20,20,20,0.75)
Border          rgba(255,255,255,0.08)
Accent          #7C5CFF
Text            #FFFFFF
Secondary       #B8B8B8
Error           #FF5C5C
```

`#7C5CFF` is Piplo's one brand difference from Saylo. Use it for the recording
ring, focus states, and the active toggle — nowhere else.

---

# Animations

Framer Motion.

**Widget morph** (chip ⇄ pill): spring, `stiffness: 400`, `damping: 32`.
**Show**: fade in, scale `0.95 → 1`, ~200 ms.
**Hide**: fade out, scale `1 → 0.95`, ~150 ms.

`transcribing` reuses the recording shape — no extra morph mid-flight, because
the user should not be able to tell there are two network calls.

---

# Project Structure

```
src/
  components/
    FloatingWidget.tsx     shape + morph
    PillContents.tsx       what's inside the pill per status
    Waveform.tsx           bars driven by `level`
    MicIcon.tsx
    WidgetMenu.tsx         the right-click menu
    Sidebar.tsx            home window nav
    HistoryRow.tsx         one dictation
    SnippetRow.tsx         one snippet, and its edit form
    Pagination.tsx         shared by history and snippets
    SettingsPanel.tsx
    ShortcutRecorder.tsx   captures a real chord
    ui/                    shadcn primitives
  pages/
    DesktopWindow.tsx      shell for the `home` window
    HomePage.tsx           history list
    SnippetsPage.tsx       triggers → canned text
    SettingsPage.tsx
  hooks/
    useWidgetEvents.ts     status + level listeners
    useHistory.ts
  store/
    appStore.ts
  lib/
    commands.ts            typed invoke wrappers
    shortcuts.ts           accelerator parsing / validation
    utils.ts

src-tauri/src/
  main.rs  lib.rs
  platform.rs   no-focus window flags
  shortcut.rs   global hotkey, hold mode
  widget.rs     show / hide / position
  menu.rs       right-click menu window
  session.rs    the state machine
  audio.rs      cpal capture, downmix, WAV
  groq.rs       transcription
  grammar.rs    cleanup
  snippets.rs   triggers, matching, snippets.json
  insert.rs     SendInput
  history.rs    JSONL
  settings.rs   settings.json
  tray.rs
```

`main.tsx` picks the root component from the current window label. One bundle,
three entry points.

---

# Coding Rules

- TypeScript everywhere. No `any` that could have been a type.
- Keep components small. Prefer composition over one big component.
- Avoid abstractions with a single call site.
- No dependency without a one-sentence reason.
- Rust: no `unwrap()` on anything that can fail at runtime. Log and degrade.
- Anything in the dictation path fails soft, never loudly.
- Comments explain **why**, not what.

---

# Build Order

Do not skip ahead — each milestone is verifiable on its own.

1. **[Shell](docs/MVP_PLAN.md#m1--shell)** — windows, tray, hotkey, widget morph. No audio.
2. **[Transcribe](docs/TRANSCRIBE.md)** — capture → Groq → type → log.
3. **[Grammar](docs/GRAMMAR.md)** — the cleanup step.
4. **[Home](docs/HOME.md)** — history list and [settings](docs/SETTINGS.md).
5. **[Right-click menu](docs/WIDGET.md#right-click-menu)** — needs the rest to exist first.
6. **[Snippets](docs/SNIPPETS.md)** — triggers, matching, and the page.

Full plan and per-milestone verification: [docs/MVP_PLAN.md](docs/MVP_PLAN.md).
