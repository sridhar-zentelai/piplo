# Snippets

Short spoken triggers that get typed as longer canned text.

```
say "my email"  →  Piplo types  codeaprogram@gmail.com
say "sign off"  →  Piplo types  Thanks,\nSridhar
```

One phrase in, one block of text out. No placeholders, no variables, no
per-app rules — see [what this is not](#what-this-is-not).

A fifth feature rather than a variation on dictation: it is the only one where
what gets typed is **not** what you said.

---

## Where it sits in the pipeline

Between [grammar](GRAMMAR.md) and [typing](TRANSCRIBE.md#insertrs), with one
check moved ahead of grammar:

```
transcript (raw)
      │
      ├─ trigger match on raw? ──── hit ──→ content ──┐
      │                                              │
      ▼ miss                                         │
   grammar.rs                                        │
      │                                              │
      ├─ trigger match on cleaned? ─ hit ──→ content ─┤
      │                                              │
      ▼ miss                                         │
   cleaned text ────────────────────────────────────►┤
                                                     ▼
                                              insert.rs + history.rs
```

**Two checks, not one.** The first, on the raw transcript, is what usually fires
— triggers are short and clean, so Whisper rarely mangles them. A hit there
**skips grammar entirely**, which removes a network call and its latency from the
most repeated action in the app. (Most triggers are under three words and would
[skip grammar anyway](GRAMMAR.md#guarding-the-output), so this mainly matters for
longer ones.)

The second check exists because grammar sometimes *fixes* a trigger into
matchability — "my e-mail" → "my email". Without it, a mishearing silently turns
a snippet into a typed sentence.

**A match replaces the whole utterance.** The user said a shorthand, not a
sentence they wanted typed.

`session.rs` stays the only module that knows this order.

---

## Matching

### Normalization

Both sides of the comparison — the spoken text and the stored trigger — go
through the same function:

1. Trim whitespace and surrounding ASCII punctuation
2. Collapse runs of inner whitespace to single spaces
3. Lowercase

Whisper decides on its own whether an utterance ends in a full stop and how it is
capitalised, so **neither can be part of the comparison**. `"My email."`,
`"  my   EMAIL  "` and `"my email"` are the same phrase.

### Whole utterance only

A trigger found *inside* a sentence does not match.

| Spoken | Trigger `my email` | Why |
| ------ | ------------------ | --- |
| "My email." | ✅ match | The whole utterance is the trigger |
| "  my   EMAIL  " | ✅ match | Normalization |
| "send my email to Bob" | ❌ no match | Far more likely to be someone *talking about* it |
| "my emails" | ❌ no match | Exact after normalization, not fuzzy |

Substring matching would make the feature unpredictable in exactly the situation
where predictability matters — text going into a real document. Exact-after-
normalization is a rule the user can hold in their head.

### Empty input never matches

An utterance that normalizes to nothing (silence, `"...!"`) matches nothing, even
if a stored trigger somehow normalizes to empty too. Guard both ends.

---

## Storage

`snippets.json` in the app config dir, **next to `settings.json`** — not in the
repo.

```json
[
  { "id": "uuid-v4", "trigger": "my email", "content": "codeaprogram@gmail.com" }
]
```

Same reasoning as [settings](SETTINGS.md#where-they-live): this is state that
should follow the install, not the checkout. It is unlike
[history](TRANSCRIBE.md#historyrs), which lives in the repo because it exists to
be read and grepped.

A corrupt file loads as **no snippets** rather than failing the launch —
dictation works fine without them, and the next save overwrites the bad file.

### Write, then adopt

The in-memory list is only replaced **after** the file write succeeds. A failed
write leaves memory and disk in agreement, both holding the last good state. The
alternative — updating memory first — gives the user a snippet that works until
they restart, which is worse than a visible error.

---

## `snippets.rs`

| Item | Purpose |
| ---- | ------- |
| `Snippet { id, trigger, content }` | `id` is a uuid v4 minted on create. Empty `id` on the way in means "this is new" |
| `SnippetsState(Mutex<Vec<Snippet>>)` | The live list, shared with the pipeline. Managed state |
| `load(app)` | Read at startup, falling back to none |
| `normalize(text)` | The rule above. **Unit-tested** |
| `match_trigger(&[Snippet], text)` | `Option<String>` — the content, if the whole utterance is a trigger. **Unit-tested** |
| `commit(app, list)` | Write, then adopt, then return the list |

`normalize` and `match_trigger` are pure functions over data — no `AppHandle`, no
I/O — which is what makes them the two things in this feature worth unit tests.
Port Saylo's test cases: case/padding/punctuation, whole-utterance-only, empty
input, and picking the right one out of several.

### Validation, in Rust

The backend is the only place that decides what is valid. The form pre-checks the
same rules for instant feedback, but never authoritatively.

| Rejected | Message |
| -------- | ------- |
| Trigger empty after normalization | "Give the snippet something to say." |
| Content empty after trimming | "Give the snippet some text to type." |
| Trigger already used by another snippet | "'{trigger}' is already a trigger." |

**The clash check is the important one.** Two snippets answering to the same
phrase makes which one wins a matter of list order — something the user never
sees and cannot control. Compare *normalized* triggers, and exclude the snippet
being edited from its own clash check or saving an unchanged row fails.

Trigger and content are trimmed before storing, so a trailing space can never be
the invisible difference between two entries.

### Commands

Every mutation returns **the whole list**, so the page replaces its state outright
rather than patching it and hoping the two stay in step.

| Command | Returns | Used by |
| ------- | ------- | ------- |
| `list_snippets` | `Vec<Snippet>` | The page, on mount |
| `save_snippet` | `Result<Vec<Snippet>, String>` | Create and update — both, keyed on `id` |
| `delete_snippet` | `Result<Vec<Snippet>, String>` | Row delete |

One command for create and update: the page has one form, the difference is
whether `id` is empty, and splitting them would put that branch in two places.

---

## The page

A third sidebar entry in the [home window](HOME.md), so `DesktopWindow.tsx`'s
page state becomes `"home" | "snippets" | "settings"`. Still no router.

```
┌──────────────┬────────────────────────────────────────────────┐
│              │  Snippets                      [+ New snippet] │
│  ◉  Piplo    │  Say a trigger while dictating and Piplo types  │
│              │  the text instead — 12 saved.                   │
│  ⌂  Home     │                                                │
│  ⧉  Snippets │  [ Search snippets        ]  [ Newest ▾ ]      │
│  ⚙  Settings │  ┌──────────────────────────────────────────┐  │
│              │  │ my email      →  codeaprogram@gmail.com  │  │
│              │  │ sign off      →  Thanks, Sridhar         │  │
│              │  │ intro email   →  Hi, I hope you're doing…│  │
│              │  └──────────────────────────────────────────┘  │
│  v0.1.0      │              ‹  1–12 of 12  ›                  │
└──────────────┴────────────────────────────────────────────────┘
```

### Row alignment

Three columns, the same grid on every row so nothing shifts:

| Column | Width | Content |
| ------ | ----- | ------- |
| Trigger | Fixed, truncating | What you say. `title` attribute carries the full value |
| Arrow | Fixed | Non-semantic, `aria-hidden` |
| Content | Fills, truncating to one line | What gets typed |

Actions (edit, delete) occupy a fixed column at the end **whether or not they are
visible**, so revealing them on hover cannot reflow the row.

### Editing

**In place, inside the row** — not a dialog. A trigger and a body are not enough
to justify taking over the window, and it matches how a history row expands to
show its full text.

- Clicking anywhere on the row starts editing; the pencil is what *says* so.
- The new-snippet form opens at the top of the list.
- Fields: trigger (single line), content (multi-line, resizable).
- `Ctrl+Enter` / `Cmd+Enter` saves from the content field — a textarea keeps
  `Enter` for newlines, so saving needs the modifier every editor already uses.
- `Escape` cancels the row, and must `stopPropagation` so it does not also reach
  whatever else listens for it.
- The form keeps its own draft state. A half-typed snippet is nobody else's
  business, and the saved list only changes once the backend accepts it.
- **A rejected save is shown inside the form**, next to the field that caused it —
  the row being edited can be far down a long list, and a page-level banner would
  be off screen.
- Typing clears the error. The user is already answering it.

### Deleting

Inline confirm in the row: the trash becomes *Delete? Delete / Cancel*. A snippet
is gone for good, so it asks once — but a modal for one row is heavier than the
action deserves. The confirm pins the action column open, or it would vanish
mid-question when the pointer moved.

### Search and sort

Both client-side over the already-loaded list. The toolbar only appears once
there are at least two snippets — below that there is nothing to search or sort.

- **Search** matches trigger *or* content, case-insensitively. `Escape` clears
  it. When searching, show `n of m` so a short list does not look like data loss.
- **Sort**: Newest · Oldest · A–Z. Newest and Oldest lean on **file order** —
  snippets are appended as they are created, so the end of the list is the most
  recent. Cheaper than a timestamp field nobody would ever see.

### Empty states

Two, and they must be distinguishable from *still loading* — a page that has not
answered yet looks exactly like a page with nothing on it. Track `loaded`
explicitly and render neither state until the first response lands.

**Nothing yet** — explains the idea, because a user with an empty list does not
know what a snippet is. Three fake examples in the trigger → content shape, plus
a *New snippet* button. The examples live here and **nowhere else**: once there
are real snippets, examples would be a second list of fake ones above the true
one.

**Nothing matched** — the search term and a *Clear search* action. Never a dead
end.

### Pagination

Same component and same rules as [history](HOME.md#pagination): 20 rows a page,
`from–to of total` readout, prev/next, only shown above one page's worth, cursor
clamped when the row count shrinks.

Clamping matters more here than on history, because the count changes from
*inside* the page — deleting the last snippet on the last page must not leave the
cursor pointing past the end.

Search and sort reset to page 1. Landing on page 3 of a fresh result set reads as
a bug.

---

## Shared with the history page

Build none of these twice. If one needs to change for snippets, it changes for
both or it is not shared.

| Element | Where it lives |
| ------- | -------------- |
| Pagination control | `Pagination.tsx` — already built for history |
| Empty-state container | Extract from `HomePage.tsx`; takes a title, an optional hint, and children |
| Row list container | The bordered list wrapper, with rows divided and the last divider dropped |
| Fixed-column row grid | The alignment rule above is history's rule with different columns |
| Inline confirm | History's clear-all confirm, scoped to a row |
| Truncate-with-`title` | Same treatment for long text in both lists |

Snippets adds **search** and **sort**, which history does not have. Keep them
local to the snippets page until something else needs them — history deliberately
has [no search](MVP_PLAN.md#deliberate-gaps).

---

## History logging

An expansion is logged like any other dictation, so the list stays a complete
record of what was typed. `text` holds **the content that was typed**, not the
trigger — that is what `text` means everywhere else and changing it here would
break the one guarantee the field has.

One added field, so an expansion is distinguishable after the fact:

```json
{"…", "text": "codeaprogram@gmail.com", "raw_text": "my email", "snippet": true}
```

Not shown in the UI. It is there for the same reason `corrected` is — so the
feature's real-world behaviour is greppable:

```bash
grep -c '"snippet":true' history/history.jsonl
```

---

## Checks

1. **It fires.** Create `my email` → your address. Dictate "my email" into
   Notepad; the address is typed, the phrase is not.
2. **Normalization holds.** "My email.", "  my   EMAIL  " and "my email" all fire.
3. **Whole utterance only.** "send my email to Bob" types the sentence, not the
   address. This is the one that keeps the feature trustworthy.
4. **Grammar is skipped on a hit.** Watch the log — a matched trigger makes one
   network call, not two.
5. **A mishearing still resolves.** Force grammar to fix a trigger and confirm the
   second check catches it.
6. **Clash is refused.** Two snippets with triggers "My Email" and "my email"
   cannot both exist. The message names the trigger.
7. **Editing a row and saving it unchanged succeeds** — it must not clash with
   itself.
8. **Empty trigger and empty content are both refused**, with the message inside
   the form.
9. **A failed write leaves nothing changed.** Make `snippets.json` read-only, try
   to save: an error, and the list is exactly as it was — including after a
   restart.
10. **Corrupt `snippets.json` by hand** → the app starts, the page is empty,
    dictation still works. The next save fixes the file.
11. **Delete asks once**, and the row is gone from the file, not just the list.
12. **Empty state explains the feature** to someone who has never used it, and is
    never visible while still loading.
13. **Pagination survives deletion.** 21 snippets, go to page 2, delete the only
    row on it → you land on page 1, not an empty page.
14. **Search then sort then paginate** in combination, without the count readout
    disagreeing with the visible rows.

---

## What this is not

Each of these is a real request and each one is out of scope. Adding any of them
turns a rule the user can hold in their head into a system they have to learn.

- **No placeholders or variables** — no `{date}`, no cursor position, no
  clipboard interpolation. Content is literal text.
- **No fuzzy or partial matching.** Exact after normalization, whole utterance.
- **No keyboard-typed expansion.** Triggers are *spoken*. Piplo is not a text
  expander and does not watch what you type; that would mean a keyboard hook and
  a completely different privacy story.
- **No per-app snippets**, no folders, no tags.
- **No import/export**, no sync. `snippets.json` is a plain file the user can copy.
- **No usage counts.** That is analytics, and it is on the
  [do-not list](../CLAUDE.md#do-not-implement).
- **No hotkey per snippet.** One global shortcut is the whole input model.
