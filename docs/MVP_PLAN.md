# Build Plan

Six milestones. Each one is independently verifiable — do not start the next
until the current one passes its checks. The order is chosen so that the riskiest
native work (focus, keystrokes) is proven before any UI is built on top of it.

| # | Milestone | Ships | Doc |
| - | --------- | ----- | --- |
| M1 | [Shell](#m1--shell) | Windows, tray, hotkey, widget morph. No audio | [WIDGET.md](WIDGET.md) |
| M2 | [Transcribe](#m2--transcribe) | Speech → text at the caret, logged | [TRANSCRIBE.md](TRANSCRIBE.md) |
| M3 | [Grammar](#m3--grammar) | Automatic cleanup before typing | [GRAMMAR.md](GRAMMAR.md) |
| M4 | [Home](#m4--home) | History list + settings | [HOME.md](HOME.md), [SETTINGS.md](SETTINGS.md) |
| M5 | [Right-click menu](#m5--right-click-menu) | The widget's menu | [WIDGET.md](WIDGET.md#right-click-menu) |
| M6 | [Snippets](#m6--snippets) | Spoken triggers → canned text, and their page | [SNIPPETS.md](SNIPPETS.md) |

---

## M0 — Scaffold

```bash
bun create tauri-app piplo --template react-ts --manager bun
```

Then:

- `bunx shadcn@latest init` — dark theme, no light palette
- Tailwind v4 via `@tailwindcss/vite` (not the PostCSS plugin)
- Fonts: `@fontsource-variable/geist`, `geist-mono`, `plus-jakarta-sans`
- `src-tauri/Cargo.toml` — add the dependency set from
  [CLAUDE.md](../CLAUDE.md#tech-stack)
- Plugins: `tauri-plugin-global-shortcut`, `tauri-plugin-window-state`
- `.gitignore` and `.env.example` are already committed. **Confirm `.env` and
  `history/` are ignored before writing a key or dictating anything** — the
  scaffold overwrites `.gitignore` if you let it
- `identifier: "com.codea.piplo"`, `productName: "Piplo"`

Release profile, to keep the binary small:

```toml
[profile.release]
opt-level = "z"
lto = true
codegen-units = 1
panic = "abort"
strip = true
```

**Check:** `bun run tauri dev` opens a default window and closes cleanly.

---

## M1 — Shell

Everything native except audio. This is where focus behaviour is proven, and it
must be proven before anything types into another app.

**Windows** — declare all three in `tauri.conf.json` per
[CLAUDE.md](../CLAUDE.md#windows). `widget` visible, `menu` and `home` hidden.

**`platform.rs`** — apply `WS_EX_NOACTIVATE` to the widget and the menu via
`SetWindowLongPtrW`. Do it in `setup`, after the windows exist.

**`tray.rs`** — icon, tooltip "Piplo", menu: *Open Piplo* / *Quit*. Left click
opens the home window.

**`shortcut.rs`** — register `Ctrl+Space`. Hold semantics: `Pressed` starts,
`Released` finishes. Windows sends auto-repeat `Pressed` events while a key is
held, so guard with a `key_down: bool` and ignore repeats.

**`widget.rs`** — `show`, `hide`, `resize`, and position (bottom-centre of the
work area, above the taskbar). Emit `status` on change.

**Frontend** — `FloatingWidget.tsx` with the shape map, `PillContents.tsx`,
`Waveform.tsx` driven by a fake sine wave for now, `MicIcon.tsx`,
`useWidgetEvents.ts`, `appStore.ts`.

**Checks**

1. Widget sits bottom-centre, above other windows, not in the taskbar or Alt-Tab.
2. Hold `Ctrl+Space` → morphs to the pill; release → back to the chip.
3. **Focus is never stolen.** Put the caret in Notepad, trigger the widget, click
   the mic button, then type on the keyboard. Every character must land in
   Notepad. If focus moves, `WS_EX_NOACTIVATE` is wrong — stop and fix it here.
4. Holding the shortcut for five seconds fires one start, not fifty.
5. Tray → Open Piplo shows the home window; closing it leaves the app running.

---

## M2 — Transcribe

The full pipeline, no cleanup. Detail in [TRANSCRIBE.md](TRANSCRIBE.md).

Order to build in — each step testable with the previous one stubbed:

1. `audio.rs` — capture, RMS → real `level`, downmix, WAV bytes
2. `groq.rs` — the multipart POST and typed errors
3. `insert.rs` — modifier wait, `SendInput` Unicode
4. `history.rs` — one JSONL line per dictation
5. `session.rs` — the state machine tying them together

**Checks**

1. Waveform tracks your voice and goes flat in silence.
2. Notepad: hold, speak a sentence, release → text at the caret within ~2 s.
3. Release `Space` but keep `Ctrl` held for a second. **Nothing happens until
   `Ctrl` is up**, then the text types normally. Menus opening or characters
   vanishing means the modifier wait is broken.
4. Copy something, dictate, paste → you get your original clipboard back.
5. Mic click → speak → ✓ types the text. Key-up must not end that session.
6. Mic click → speak → ✕ types nothing, logs nothing, and nothing appears late.
7. A 200 ms accidental tap makes no API call.
8. `history/history.jsonl` gains one plausible line per success.
9. Unset `GROQ_API_KEY` → the pill shows the error and returns to idle. No crash,
   no hang.

---

## M3 — Grammar

One Rust module and one insertion into `session::deliver`. Detail in
[GRAMMAR.md](GRAMMAR.md).

**Checks**

1. *"um so i think we should uh ship it monday and then like tell the team
   after"* → `So I think we should ship it Monday, and then tell the team after.`
2. *"what is the capital of France"* → the **question**, punctuated. Getting
   "Paris" means the guards failed; do not proceed until they don't.
3. An opinionated or oddly-worded sentence survives intact.
4. `PIPLO_GRAMMAR_MODEL=does/not-exist` → raw text still types, 404 in the log.
   **This is the most important test in the milestone** — it is the difference
   between a feature and a liability.
5. Timeout set to 1 ms temporarily → every dictation types raw, no visible error.
6. `PIPLO_GRAMMAR=0` types exactly what Whisper returned.
7. No `<think>` or preamble text ever reaches the document.
8. JSONL has both `text` and `raw_text`, and they differ on messy dictations.

---

## M4 — Home

The only real UI work. Detail in [HOME.md](HOME.md) and
[SETTINGS.md](SETTINGS.md).

- `DesktopWindow.tsx` + `Sidebar.tsx` — two entries, Home and Settings
- `HomePage.tsx` — history list, newest first, copy per row, clear all
- `SettingsPage.tsx` + `SettingsPanel.tsx` — the three settings
- `ShortcutRecorder.tsx` — captures a real chord, `event.code` not `event.key`
- `settings.rs` — load, validate, save, and apply live

**Checks**

1. Dictate, then open Home → the new entry is at the top.
2. Copy on a row puts the text on the clipboard.
3. Change the shortcut → the new chord works immediately, the old one does not,
   and it survives a restart.
4. A shortcut already taken by another app is **rejected** and the previous
   binding still works. Losing the binding entirely is a failure.
5. Grammar toggle off → the very next dictation types raw text. No restart.
6. Widget toggle off → the chip disappears but the shortcut still summons the
   pill for the duration of a session.
7. Delete `settings.json` while running, restart → defaults, no crash.
8. Corrupt `settings.json` by hand → defaults, no crash.

---

## M5 — Right-click menu

Last, because every item it contains has to exist first. Detail in
[WIDGET.md](WIDGET.md#right-click-menu).

Items: *Dictate* · *Grammar correction* (toggle) · *Open Piplo* · *Quit*.

**Checks**

1. Right-click the widget → menu appears adjacent to it, on screen even when the
   widget is near an edge.
2. Clicking outside, or `Escape`, dismisses it.
3. The menu **does not steal focus** — same Notepad test as M1.
4. Toggling grammar in the menu is reflected on the settings page next time it
   is opened, and vice versa.
5. *Dictate* starts a session with no keyboard involved; ✓ ends it.
6. *Quit* exits cleanly with no orphaned tray icon.

---

## M6 — Snippets

Say a saved trigger on its own and Piplo types the canned text instead. Detail in
[SNIPPETS.md](SNIPPETS.md).

Build order — the matching rules are pure functions, so they are testable before
any UI exists:

1. `snippets.rs` — `normalize` and `match_trigger`, with the unit tests. No
   wiring yet
2. Load, save, `commit` (write then adopt), and the three commands
3. The two calls in `session::deliver` — on raw before grammar, on cleaned after
4. `SnippetsPage.tsx` + `SnippetRow.tsx`, reusing the pagination control, the
   empty-state container and the row grid from the history page
5. The third sidebar entry

**Checks** — the full list is in
[SNIPPETS.md](SNIPPETS.md#checks). The four that matter most:

1. **Whole utterance only.** "send my email to Bob" types the sentence, not the
   address. Substring matching would make the feature untrustworthy in documents.
2. **Clash is refused.** "My Email" and "my email" cannot both exist, and editing
   a row without changing its trigger still saves.
3. **A failed write changes nothing.** Make `snippets.json` read-only and try to
   save: an error, and the list is unchanged — including after a restart.
4. **Corrupt `snippets.json` by hand** → the app starts, the page is empty, and
   dictation still works.

---

## Done

Piplo is finished when M1–M6 pass. The [do-not-implement
list](../CLAUDE.md#do-not-implement) is not a backlog — it is the boundary.

## Deliberate gaps

Known and accepted, not oversights:

- **No conflict detection** against other apps' hotkeys beyond what Windows
  reports at registration. Windows lets some combinations register and then
  swallows the keypress.
- **No duration cap** on recording. Groq's limit is 25 MB (~13 min at 16 kHz
  mono); a long dictation will fail with an API error rather than a clean
  message.
- **No history search.** The list is paginated at 20 rows but still read whole
  into memory, so the *read* becomes the cost in the thousands of entries, not the
  hundreds. The fix at that point is a windowed read, not a database.
- **Unicode `SendInput` can be dropped** by games and some DirectInput apps. The
  fallback would be clipboard + `Ctrl+V`; the seam for it is `insert.rs`.
