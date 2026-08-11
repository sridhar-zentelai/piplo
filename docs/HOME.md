# Home Window

The `home` window is the only conventional window in the app: decorated,
resizable, 880×600, and **hidden on startup**. It exists to answer one question —
"what did I dictate?" — and to hold the three settings.

Opened from the tray or the widget's right-click menu. Closing it hides it; the
app keeps running in the tray.

```
┌──────────────┬──────────────────────────────────────────────┐
│              │  History                                     │
│  ◉  Piplo    │                          142 dictations  ⌫   │
│              │  ┌────────────────────────────────────────┐  │
│  ⌂  Home     │  │ So I think we should ship it Monday…   │  │
│  ⚙  Settings │  │ 2 minutes ago · 4.2s · en          ⧉  │  │
│              │  ├────────────────────────────────────────┤  │
│              │  │ Can you send me the deck when you…     │  │
│              │  │ 18 minutes ago · 6.8s · en         ⧉  │  │
│              │  └────────────────────────────────────────┘  │
│  v0.1.0      │                                              │
└──────────────┴──────────────────────────────────────────────┘
```

Two pages, one sidebar. No router — `DesktopWindow.tsx` holds a
`useState<"home" | "settings">`. A router for two views with no URLs and no deep
linking would be an abstraction with a single call site.

---

## Layout

| Component | Responsibility |
| --------- | -------------- |
| `DesktopWindow.tsx` | Shell, page state, sidebar + content split |
| `Sidebar.tsx` | Brand mark, two nav entries, version at the bottom |
| `HomePage.tsx` | The list, the count, clear-all |
| `HistoryRow.tsx` | One entry |
| `SettingsPage.tsx` | See [SETTINGS.md](SETTINGS.md) |

Sidebar is a fixed 200 px; the content area scrolls. `minWidth: 520`,
`minHeight: 400` in the window config so the split cannot be squeezed into
nonsense.

Dark only. There is no theme switch — see
[CLAUDE.md](../CLAUDE.md#ui-principles).

---

## History

Read from `history/history.jsonl` via `get_history`, newest first.

### A row

- **The text** — up to three lines, then ellipsised. Click to expand in place;
  no modal, no detail page.
- **Relative time** — "2 minutes ago", "yesterday", then an absolute date past a
  week. Computed on render from `at`.
- **Duration** and **language** from the transcription metadata.
- **Copy** button → `@tauri-apps/plugin-clipboard-manager`.

**Copy must use the plugin, not `navigator.clipboard`.** The packaged app serves
`http://tauri.localhost`, which is not a secure context, so the browser API is
unavailable — and it works fine under `tauri dev`, so this breaks only in the
build if you get it wrong.

`raw_text` is not shown. It is in the file for auditing the
[grammar step](GRAMMAR.md#logging-both-versions), not for the user to compare
side by side — that would be a diff viewer, which is not one of the four
features.

### Loading

`useHistory` fetches once on mount and re-fetches when the window is shown
(`Focused(true)`). Dictations happen while this window is hidden, so a stale list
is the normal case, not an edge case.

No live push from Rust. The window is usually hidden; emitting into a hidden
webview on every dictation is work nobody sees.

### Empty state

Centred, quiet: a mic glyph, "No dictations yet", and the current shortcut
rendered in a `Kbd`. It is the first thing a new user sees, so it should tell
them how to start.

### Clear all

One destructive action, in the header, behind a confirm. Truncates the file via
`clear_history`.

**Use a shadcn dialog, never `window.confirm`.** A native modal inside the
webview blocks the whole window and looks nothing like the app.

### Scale

The file is read whole and the list rendered whole — no pagination, no search,
no virtualisation. At a few hundred entries this is instant. At several thousand
it will not be, and the fix at that point is virtualisation, not a database.
Recorded as a [deliberate gap](MVP_PLAN.md#deliberate-gaps).

---

## Commands

| Command | Returns |
| ------- | ------- |
| `get_history` | `Vec<Entry>`, newest first |
| `clear_history` | `()` |
| `open_home` | `()` — show + focus + unminimise |

`open_home` must handle the window already being open but behind something else:
`show()`, `unminimize()`, then `set_focus()`. Only calling `show()` on a window
that is already shown does nothing visible, which reads as a broken tray icon.

This is the one place in the app that *should* take focus. The widget must never;
the home window is a normal window the user just asked for.
