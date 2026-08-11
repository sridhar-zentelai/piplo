# Tasks

[MVP_PLAN.md](MVP_PLAN.md) says what each milestone ships and how to know it
works. This file is the same ground at a smaller scale: **one task per session**,
each ending in something you can look at or try.

A milestone is too big to build in one sitting. M1 alone is three windows, a
tray, a global hotkey and an animation — four days of work behind a single
checkbox. These are the checkboxes in between.

Work top to bottom. Don't start a task until the one above it is ticked.

> The **See** line is the point of each task. If you can't see it, the task
> isn't done — and if a task has no way to be seen, it was written wrong. Split
> it.

---

## M0 — Scaffold

- [x] **0.1 — The app opens**
  `bun create tauri-app`, Tailwind v4 via `@tailwindcss/vite`, shadcn init
  (dark only), the three fonts, and the release profile from
  [MVP_PLAN.md](MVP_PLAN.md#m0--scaffold). Identifier `com.codea.piplo`.
  **Before anything else:** confirm `.env` and `history/` survived in
  `.gitignore` — the scaffold overwrites it.
  **See:** `bun run tauri dev` opens a window and closes cleanly.

- [x] **0.2 — Colors and fonts are Piplo's**
  The palette from [CLAUDE.md](../CLAUDE.md#colors) as CSS variables, `#7C5CFF`
  included. No light palette anywhere.
  **See:** a throwaway page in the default window showing each color and font.
  Delete it at the end of the task.

## M1 — Shell

- [x] **1.1 — Three windows**
  `widget`, `menu`, `home` in `tauri.conf.json` per
  [CLAUDE.md](../CLAUDE.md#windows). Widget visible, other two hidden.
  `main.tsx` picks its root component from the window label.
  **See:** a 56×56 transparent square floating above everything, nothing in the
  taskbar, nothing in Alt-Tab.

- [ ] **1.2 — The widget never takes focus** ← *the load-bearing one*
  `platform.rs`: `WS_EX_NOACTIVATE` on `widget` and `menu` via
  `SetWindowLongPtrW`, in `setup`, after the windows exist.
  **See:** caret in Notepad, click the widget, type — every character lands in
  Notepad.
  **If it doesn't, stop.** Everything after this assumes focus stays put, and
  no amount of UI on top will fix it.

- [x] **1.3 — It sits in the right place**
  `widget.rs`: bottom-centre of the work area, above the taskbar. Add
  `tauri-plugin-window-state` for position only.
  **See:** correct on launch; still correct after a resolution change.

- [x] **1.4 — Tray**
  Icon, tooltip, *Open Piplo* / *Quit*. Left click opens `home`. Closing `home`
  hides it rather than quitting.
  **See:** the icon in the tray; open and close the window three times; the app
  is still alive.

- [x] **1.5 — Hold to talk**
  `shortcut.rs`: register `Ctrl+Space`, `Pressed` starts, `Released` finishes.
  Windows auto-repeats `Pressed` while held — guard with `key_down: bool`.
  **See:** hold five seconds; the log prints one start and one stop, not fifty.

- [x] **1.6 — Chip becomes pill**
  `FloatingWidget.tsx` + `PillContents.tsx` + `MicIcon.tsx` + `appStore.ts` +
  `useWidgetEvents.ts`. Spring `stiffness: 400`, `damping: 32`.
  **See:** hold the shortcut, the chip stretches into the pill; release, it
  goes back. No flicker at either end.

- [x] **1.7 — Waveform**
  `Waveform.tsx`, driven by a fake sine wave through the `level` event so the
  wiring is real even though the number isn't.
  **See:** bars moving inside the pill while held.

> M1 is done when the five checks in
> [MVP_PLAN.md](MVP_PLAN.md#m1--shell) pass. Run them.

## M2 — Transcribe

Each step below works with the next one stubbed, so each is visible on its own.

- [x] **2.1 — Real audio**
  `audio.rs`: `cpal` capture on a worker thread, downmix to mono, RMS → the
  `level` event at ~30 Hz. Replace the sine wave.
  **See:** the waveform tracks your actual voice and goes flat when you stop.

- [x] **2.2 — A WAV that plays**
  Downsample to 16 kHz, encode with `hound`, write it to disk temporarily.
  **See:** open the file in a player and hear yourself clearly.
  Drop the disk write once it sounds right.

- [x] **2.3 — Groq returns words**
  `groq.rs`: the multipart POST, typed errors, no `unwrap`.
  **See:** speak, and your sentence prints in the terminal.

- [x] **2.4 — Words reach the caret**
  `insert.rs`: wait for modifiers to be released, then Unicode `SendInput`.
  **See:** Notepad — hold, speak, release, text appears.
  **Then:** release `Space` but keep `Ctrl` down for a second. Nothing must
  happen until `Ctrl` is up. Menus opening means the wait is broken.

- [x] **2.5 — History file**
  `history.rs`: one JSONL line per dictation, in `history/`.
  **See:** `history/history.jsonl` gains a plausible line per dictation, and
  `git status` stays clean.

- [x] **2.6 — The state machine**
  `session.rs`: the only module that knows the order. Mic click starts, ✓ ends,
  ✕ discards. A tap under ~300 ms makes no API call.
  **See:** all six of these by hand — key hold, mic click + ✓, mic click + ✕
  (nothing types, now or late), a quick tap (no network call in the log), a
  dictation with no API key (the pill shows the error, then returns to idle),
  and the clipboard you had before is still there afterwards.

> Run all nine checks in [MVP_PLAN.md](MVP_PLAN.md#m2--transcribe) before M3.
> This is the milestone everything else sits on.

## M3 — Grammar

- [x] **3.1 — Cleanup happens**
  `grammar.rs` and its one call in `session::deliver`. Both `text` and
  `raw_text` into the JSONL.
  **See:** *"um so i think we should uh ship it monday and then like tell the
  team after"* becomes
  `So I think we should ship it Monday, and then tell the team after.`

- [x] **3.2 — It can't cost you a dictation** ← *the one that matters*
  Timeout, the answered-instead-of-cleaned guard, `<think>` stripping, preamble
  stripping, length bounds. Every failure path types the raw transcript.
  **See:** `PIPLO_GRAMMAR_MODEL=does/not-exist` — your words still type, and a
  404 appears in the log.
  **Then:** dictate *"what is the capital of France"* and get the question
  back, punctuated. Getting "Paris" means the guards don't work yet.

> Eight checks in [MVP_PLAN.md](MVP_PLAN.md#m3--grammar). Number 4 is the
> difference between a feature and a liability — don't wave it through.

## M4 — Home

- [x] **4.1 — The window has a shape**
  `DesktopWindow.tsx` + `Sidebar.tsx`, two entries, dark, no theme switch.
  **See:** tray → Open Piplo → a window you'd be happy to screenshot, with both
  pages empty.

- [x] **4.2 — History list**
  `useHistory.ts`, `HistoryRow.tsx`, newest first, copy per row, clear all.
  **See:** dictate, open Home, your sentence is at the top; copy puts it on the
  clipboard.

- [x] **4.3 — Two settings that apply live**
  `settings.rs` (load, validate, save) + `SettingsPanel.tsx` for grammar
  on/off and widget visible.
  **See:** turn grammar off, dictate — the *very next* dictation types raw. No
  restart.
  **Then:** delete `settings.json` while it's running and restart; corrupt it
  by hand and restart. Defaults both times, no crash.

- [x] **4.4 — Rebind the shortcut**
  `ShortcutRecorder.tsx` using `event.code`, plus `shortcut.rs` rebind that
  re-registers *before* saving.
  **See:** set `Alt+Shift+D`, it works immediately, `Ctrl+Space` doesn't, and
  it survives a restart.
  **Then:** try to bind something Windows already owns — it's rejected and your
  previous shortcut still works. Losing the binding is a failure, not a
  trade-off.

## M5 — Right-click menu

- [ ] **5.1 — A menu appears**
  `menu.rs` + `WidgetMenu.tsx`. Positioned next to the widget, flipped when the
  widget is near a screen edge. Dismissed by `Escape` or a click outside.
  **See:** right-click the widget in all four corners of the screen — the menu
  is fully on screen every time.

- [ ] **5.2 — The items work**
  *Dictate* · *Grammar correction* · *Open Piplo* · *Quit*.
  **See:** *Dictate* runs a whole session with no keyboard; the grammar toggle
  matches the settings page in both directions; *Quit* leaves no orphaned tray
  icon.
  **And:** the Notepad focus test from 1.2, on the menu this time.

## Done

- [ ] **6.1 — The full sweep**
  Every check in [MVP_PLAN.md](MVP_PLAN.md), M1 through M5, in one sitting on a
  release build.
  **See:** a list of what failed. Fix those, then Piplo is finished.

---

## Rules

- **One task per session.** Stop when the **See** line is true.
- A task that can't be seen without building three other things is two tasks.
  Split it here first, then do the first half.
- The [do-not-implement list](../CLAUDE.md#do-not-implement) is not a backlog.
  If a task starts growing toward it, the task is wrong.
