# Piplo

A lightweight Windows dictation assistant. Hold `Ctrl+Space`, speak, release —
clean text is typed into whatever app you were already in.

```
hold Ctrl+Space  →  speak  →  release
                                 ↓
                    Groq Whisper transcribes
                                 ↓
                    a Groq chat model fixes the grammar
                                 ↓
                    text typed at your caret, and logged
```

Four features, on purpose:

- **Transcribe** — global shortcut, floating widget, text at the caret
- **Grammar** — automatic cleanup before typing; never rewords, never blocks
- **Home** — a window listing every past dictation, newest first
- **Settings** — shortcut, grammar on/off, widget visible

Nothing else. See [CLAUDE.md](CLAUDE.md#do-not-implement) for the list of things
that are deliberately not here.

---

## Requirements

- Windows 10/11
- [Bun](https://bun.sh)
- [Rust](https://rustup.rs) (stable, MSVC toolchain)
- WebView2 (preinstalled on Windows 11)
- A [Groq API key](https://console.groq.com/keys)

## Setup

```bash
bun install
cp .env.example .env      # then paste your GROQ_API_KEY
bun run tauri dev
```

The app starts in the tray with the widget on screen. Hold `Ctrl+Space` in any
text field and speak.

If the waveform stays flat while you talk, Windows is blocking the microphone:
**Settings → Privacy & security → Microphone → Let desktop apps access your
microphone.**

## Build

```bash
bun run tauri build
```

Output lands in `src-tauri/target/release/bundle/`.

---

## Environment

| Variable | Required | Purpose |
| -------- | -------- | ------- |
| `GROQ_API_KEY` | yes | Both the transcription and grammar calls |
| `PIPLO_GRAMMAR_MODEL` | no | Override the grammar model id |
| `PIPLO_GRAMMAR` | no | `0` bypasses grammar entirely — for debugging |

The key is read in Rust only and never reaches the webview. `.env` is
gitignored; `.env.example` holds the names.

---

## Where your data lives

| What | Where |
| ---- | ----- |
| Dictation history | `history/history.jsonl` in the repo, **gitignored** |
| Settings | `settings.json` in `%APPDATA%\com.codea.piplo` |

History sits inside the repository so it is easy to open and grep during
development, but `history/` is in `.gitignore` — the file contains everything you
have ever dictated and must never be committed.

Being ignored rather than absent has a consequence worth knowing: the file is
**local to this clone**. Deleting the working tree deletes your history, and it
does not follow you to another machine.

---

## Documentation

| Doc | Contents |
| --- | -------- |
| [CLAUDE.md](CLAUDE.md) | Scope, stack, rules. Read first |
| [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) | Module map and data flow |
| [docs/MVP_PLAN.md](docs/MVP_PLAN.md) | Build order, milestone by milestone |
| [docs/WIDGET.md](docs/WIDGET.md) | The widget, its states, the right-click menu |
| [docs/TRANSCRIBE.md](docs/TRANSCRIBE.md) | Capture → Groq → keystrokes |
| [docs/GRAMMAR.md](docs/GRAMMAR.md) | The cleanup step and its guardrails |
| [docs/HOME.md](docs/HOME.md) | Home window and history |
| [docs/SETTINGS.md](docs/SETTINGS.md) | The three settings |
