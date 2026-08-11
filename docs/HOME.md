# Home Window

The `home` window is the only conventional window in the app: decorated,
resizable, 880×600, and **opened on launch**. It exists to answer one question —
"what did I dictate?" — and to hold the three settings.

Also opened from the tray or the widget's right-click menu. Closing it hides it
rather than quitting; the app keeps running in the tray, and the tray brings it
back.

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
| `HomeHero.tsx` | Greeting, the live shortcut, status, three stat tiles |
| `Pagination.tsx` | Page cursor for the history list |
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

## The hero

Above the list: the app mark, a greeting with the version, the **current**
shortcut rendered as `Kbd` chips read from settings, and a live status dot.

The status comes from the same `status` event the widget listens to, so it says
Recording or Working during a dictation rather than guessing.

Three tiles — dictations, total words, day streak — all derived from
`history.jsonl` on render. Nothing is tracked, stored or sent anywhere; this is
arithmetic over a local file, not the analytics on the
[do-not list](../CLAUDE.md#do-not-implement).

The day streak counts consecutive local days back from today, and tolerates a gap
*today* so the number does not reset the moment midnight passes.

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

### Pagination

20 rows a page, with a `from–to of total` readout and prev/next. The scroller
returns to the top on a page change — arriving halfway down the next page reads
as a broken jump. The control only appears above one page's worth.

The page cursor is clamped whenever the row count shrinks, so clearing history
or a re-fetch that returned fewer rows cannot leave it pointing past the end.

### Scale

The file is still read whole and the whole list held in memory; pagination only
bounds what is *rendered*. At a few hundred entries this is instant. At several
thousand the read itself becomes the cost, and the fix at that point is a windowed
read, not a database. No search.

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
