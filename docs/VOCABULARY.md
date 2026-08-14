# Vocabulary

The words Piplo keeps getting wrong, and the corrections it remembers.

```
you say        "I need to check ZentelAI"
Whisper hears   I need to check gentle AI.
Piplo types     I need to check ZentelAI.
```

**Correct it once. Piplo remembers it.**

A sixth feature rather than a variation on [grammar](GRAMMAR.md): grammar fixes
how a sentence is *written*, vocabulary fixes which *words* it contains. One is a
model's judgement, the other is a lookup table the user owns.

---

## Two halves that look like one feature

They are stored together and shown together, and they are not the same thing.

| | What it is | What it does |
| - | ---------- | ------------ |
| **Term** | `ZentelAI` | Told to Whisper before the audio, so it has a chance of hearing it right |
| **Variant** | `gentle AI` → `ZentelAI` | Replaced in the transcript afterwards, so it is right even when Whisper didn't |

A term with no variants is still worth having, and a variant with no term is
meaningless — so a variant always belongs to a term, and the entry is the term.

The **variant list is where the value is**. A term is a hint that Whisper may
ignore; a variant is a deterministic replacement that always fires. This is why
[learning](#learning) writes variants and never invents terms.

Storing them together is a deliberate choice, not an accident of the schema. The
two halves are what give a single entry both layers at once: `ZentelAI` steers
Whisper *and* `gentle AI` → `ZentelAI` catches it when the steering fails. Split
into separate vocabulary and replacement records, every learned correction would
have to create two rows and keep them in sync, and the [undo](#undoing-a-fix-in-the-app-you-are-typing-in)
would have no single thing to undo.

### Off

An entry can be switched off. It keeps its variants, its provenance and its place
in the list, and takes no part in any dictation — **neither half**. No hint, no
replacement, no mention in the grammar prompt.

This exists because the alternative users reach for is deleting the entry, and a
term is expensive to rebuild: the variants on it were earned three corrections at
a time. "This is wrong in the document I am writing today" should not cost that.

---

## Where it sits in the pipeline

Two applications of one pure function, on either side of grammar, plus a hint
sent with the audio:

```
                  terms ──→ prompt hint
                              │
                              ▼
                          groq.rs
                              │ raw transcript
                              ▼
                      vocabulary::apply      ← variants → terms
                              │
                      snippet trigger? ──── hit ──→ content ──┐
                              │ miss                          │
                              ▼                               │
                         grammar.rs         ← told to preserve │
                              │                the terms it saw│
                              ▼                               │
                      vocabulary::apply      ← again, cheaply  │
                              │                               │
                      snippet trigger? ───── hit ──→ content ──┤
                              │ miss                          │
                              ▼                               ▼
                         final text ───────────────→ insert.rs + history.rs
```

Three placements, each for its own reason:

**Before the snippet check**, because a trigger can contain a term. With a
snippet on `zentelai standup`, a mishearing of the brand name would otherwise
turn the whole shorthand into a typed sentence.

**Before grammar**, so the cleanup model sees the right words. Given "gentle AI"
it will happily punctuate around the mistake and cement it.

**After grammar**, because the cleanup sometimes puts it back — capitalisation
drift on an unfamiliar token is the common one, `ZentelAI` → `Zentel AI`. The
second pass costs nothing: it is an in-memory scan over a list of a few dozen
strings, not a network call. Symmetric with the
[snippet check](SNIPPETS.md#where-it-sits-in-the-pipeline), and for the same
reason.

`session.rs` stays the only module that knows this order.

---

## Applying

`apply(&[Entry], text) -> (String, bool)` — the corrected text, and whether
anything changed. A pure function over data. **Unit-tested**, because this is the
half that runs on every dictation.

The rules, in order:

1. **Longest variant first.** `gentle AI studio` must win over `gentle AI`, and
   list order must never decide it.
2. **Whole words only.** `verbal` → `Vercel` must not fire inside `verbally`. A
   match is a match only when the characters either side are not alphanumeric.
3. **Case-insensitive on the way in, exact on the way out.** The stored term is
   written verbatim — that is the entire point of `ZentelAI` and `Next.js`.
4. **One pass, left to right, no rescanning.** A replaced span is skipped, so a
   term that happens to contain another term's variant cannot cascade.
5. **Nothing else is touched.** No spacing changes, no punctuation, no casing of
   surrounding words.

No regex. Whole-word matching over lowercased text is a scan with two boundary
checks, and a dependency needs a better reason than saving twenty lines.

### Why not fuzzy

`gentle AI` and `ZentelAI` are not close in any edit distance worth trusting, and
the pairs that *are* close — `verbal`/`Vercel`, `prism`/`Prisma` — are exactly
the ones where a false positive replaces a legitimate word in someone's document.

The variants Piplo has are better than anything a phonetic algorithm would guess,
because they are what Whisper **actually said**, recorded from a real dictation.
Learning fills the list; guessing is not needed and is not safe.

---

## The prompt hint

Groq's transcription endpoint takes a `prompt` field. Terms go into it as a plain
comma-separated list, most recently added first, capped at **180 characters**.

```
prompt: "ZentelAI, Piplo, Tauri, shadcn, Next.js, Prisma, Supabase"
```

This is free — no extra call, no extra latency — and it is the only mechanism
that can make Whisper produce the right token in the first place, before there is
any variant to replace.

**It is a hint and nothing more.** Whisper's prompt conditioning is weak and
undocumented in its effect; it may do nothing. Never treat it as the mechanism —
the [deterministic replacement](#applying) is the mechanism, and it must be able
to fix everything on its own.

Two guards, because a prompt can leak into the output on a near-silent take:

- No `prompt` field at all when the dictionary is empty.
- A transcript that normalizes to the prompt, or to a prefix of it, is treated as
  nothing transcribed. `MIN_PEAK` in `session.rs` already drops the silent takes
  that cause this; the guard is for the rest.

The 180-character cap is deliberate. Whisper's prompt window is small, a long
list is more likely to be echoed than obeyed, and the terms a user just added are
the ones they are about to say.

### Order, once the cap starts biting

Whole terms only — the cap never cuts one in half, because half a term is a word
Whisper has never seen. So on a dictionary larger than 180 characters, **order
decides who is sent at all**, and what falls off the end may as well not exist.

Two bands, newest-first inside each:

| Band | What is in it |
| ---- | ------------- |
| Starred (`priority: 1`) | Terms the user says every day |
| Everything else (`priority: 0`) | Newest first, as before |

The sort is stable, so newest-first survives inside each band. Starring is the
only way to override recency, and it is deliberately the *only* knob: a numeric
priority field the user has to reason about is a worse answer than a star.

---

## Keeping grammar's hands off

The terms that **actually appear in this transcript** are appended to the grammar
system prompt:

```
Preserve these terms exactly as written: ZentelAI, Next.js.
```

Only the ones present, never the whole dictionary — a list of words the model has
not seen in the text is an invitation to insert them.

This is a second line of defence, not the first. The
[second apply pass](#where-it-sits-in-the-pipeline) is what actually guarantees
the result; the prompt line just reduces how often it has to do the work.

---

## Learning

The product idea is "correct it once". The question is *where Piplo can see the
correction*, and the honest answer constrains the whole feature.

### The seam

**Piplo learns from corrections made in Piplo.** A history row can be edited; the
edit is compared with what was typed; a mapping is extracted.

```
┌──────────────────────────────────────────────────────┐
│  I need to check gentle AI.                          │
│  2 minutes ago · 3.1s · en              ✎  ⧉         │
└──────────────────────────────────────────────────────┘
                      ↓  fix it here
┌──────────────────────────────────────────────────────┐
│  I need to check ZentelAI.                     ✓  ✕  │
└──────────────────────────────────────────────────────┘
                      ↓
              gentle AI → ZentelAI          (seen once)
```

### Reading the focused field

There are two ways a correction reaches Piplo, and they cost very different
things.

The safe one, and the original one: **the user edits a history row** in the home
window. That is an explicit act in Piplo's own UI, it needs no permission and no
API, and it is unambiguous evidence that a transcript was wrong. It still works
and it is unchanged.

But it asks for something people do not naturally do. The text is already in their
document. They fix it *there*, because that is where it is wrong — and then Piplo
never finds out. A learning feature that only fires when the user goes somewhere
else to retype something they have already fixed is a feature that mostly does not
fire.

So there is a second way, and it is deliberately the narrowest thing that could
work:

> **One read of the focused field, at the moment a recording starts, compared only
> against the text Piplo itself last typed.**

Every clause in that sentence is load bearing.

| Clause | What it rules out |
| ------ | ----------------- |
| **One read** | No polling, no timer, no watcher. The call happens and the thread ends |
| **At the moment a recording starts** | Not while you work, not in the background, not on a schedule. There is exactly one call site: `session::start` |
| **The focused field** | Not the window title, not the file on disk, not the surrounding application. The element with keyboard focus, and only its text |
| **Compared against what Piplo typed** | The field text is useless on its own and is treated that way — it is dropped the moment the comparison is done, never logged, never stored, never sent anywhere. If Piplo's own last insertion is not found inside it, nothing happens at all |

And there is still **no keyboard hook**, here or in
[snippets](SNIPPETS.md#what-this-is-not). Piplo does not see a single keystroke
you type.

A password field is skipped before its value is touched —
`IUIAutomationElement::CurrentIsPassword` is checked first, because that is the
one field where being wrong is a disclosure rather than a bug.

**This is a real change to the boundary and it is worth being honest about.** The
old rule was "no accessibility read of another application", full stop, and it was
easier to defend precisely because it had no exceptions. The exception buys the
feature its whole reason to exist; the compensation is that the exception is
stated as a mechanism rather than an intention, so it can be checked. There is one
function — `platform::focused_text` — one caller, and no way to reach it from
anywhere else in the tree.

### What the read-back learns, and what it refuses to

Only the word. Given:

```
Piplo typed   I am working on gentle age.
the field now Some earlier notes.
              I am working on ZentelAI.
```

it adds **`ZentelAI`** to the dictionary, with no variants, and stops.

It does *not* write `gentle age → ZentelAI`. A single sighting of a mishearing is
evidence that the word exists — it is not a rule about how the word will be
misheard next time, and Whisper rarely mishears it the same way twice. A
[term on its own is already a complete entry](#the-prompt-hint): it goes into
Whisper's prompt, which is the mechanism that stops the mistake happening again at
all. Guessing at a replacement rule would add a way to be wrong without adding a
way to be right.

Nothing about this path is fuzzy. Locating Piplo's insertion inside the field is
exact word-run matching at each end (`learn::region`) — string equality, nothing
else. No edit distance, no phonetics, no scoring, no model. Where the text cannot
be located, or where the located span is too wide to be a name, the answer is
silence.

**One correction is enough here**, where the history-row path needs
[three](#confidence-is-a-count). The evidence is better: the user did not merely
retype the word, they left it standing in their own document.

### Extracting the mapping

Trim the common prefix and the common suffix, word by word. What is left in the
middle is the candidate.

```
typed   I worked with │ gentle AI │ on Monday
edited  I worked with │ ZentelAI  │ on Monday
                        ↑ candidate: gentle AI → ZentelAI
```

There is no diff algorithm here and there does not need to be one. Prefix/suffix
trimming yields the single minimal span covering **every** change, which means
two edits far apart produce one very wide span — and a wide span is rejected by
the next rule anyway. One rule does two jobs.

```
typed   we shipped │ yesterday and told the team │
edited  we shipped │ last night and told the crew│
                     ↑ five words wide — rejected
```

### The eligibility filter

This is the part that decides whether Piplo is useful or whether it slowly fills
with garbage. Every rule exists to refuse one specific kind of bad learning.

| Rule | Refuses |
| ---- | ------- |
| Both sides non-empty | Pure insertions and deletions — writing, not correcting |
| Each side ≤ **3 words** | Rewrites, and two unrelated edits in one line |
| Neither side more than 2× the other in words | "very good" → "excellent, genuinely first rate" |
| The replacement **looks like a term** | "yesterday" → "last night", "he go" → "he goes" |
| The row is not a [snippet expansion](SNIPPETS.md) | Editing canned text is editing the snippet, not the transcript |
| The pair has not been rejected before | A candidate the user already said no to |

**Looks like a term** is the load-bearing one. At least one of:

1. An **internal capital** — `ZentelAI`, `MongoDB`, `PostgreSQL`.
2. A **digit** — `S3`, `qwen3`.
3. An internal `.`, `-`, `_` or `/` — `Next.js`, `shadcn-ui`.
4. A **capital the original did not have**, on a word that is not the first in
   the utterance — `prism` → `Prisma`, `verbal` → `Vercel`.

Rule 4 does most of the work and needs no word list. Capitalisation is what
separates a proper noun from a content edit, and it is precisely what the user is
correcting when they fix a brand name. It also rejects the three edit types that
must never be learned: "yesterday" → "last night", "he go" → "he goes", and "very
good" → "excellent" all fail it.

Whisper capitalises sentence starts on its own, which is why the first word does
not count.

**The cost is accepted.** An all-lowercase name like `shadcn` will not be
auto-learned. It is still recorded, still offered as a
[suggestion](#suggestions), and can be typed in by hand in ten seconds. Piplo
would rather miss `shadcn` than learn `excellent`.

### Confidence is a count

The source plan asks for a confidence float. A float computed from a count is a
number nobody can act on, so this stores the count.

| Times seen | State | What happens |
| ---------- | ----- | ------------ |
| 1 | **observed** | Recorded. Nothing visible, nothing applied |
| 2 | **candidate** | Appears under [Suggested](#suggestions), with *Add* and *Never* |
| 3 | **learned** | Added as a variant automatically, and the row says so |
| — | **rejected** | *Never*, or deleting a learned variant. Never counted again |

This table is the **history-row** path only. The
[read-back](#reading-the-focused-field) does not use the ledger at all: it learns
a bare term on the first correction and writes nothing here, so a word learned
that way never becomes a variant and never appears as a suggestion.

Three, because two is a coincidence and four is a user who has given up. The
first occurrence is deliberately silent: a single correction is the most likely
one to be a typo or a one-off name.

**Undo is delete.** Removing a learned variant also writes the rejection, so it
does not come back on the next correction — which is the first question anyone
asks after deleting one.

### The learner never runs in the dictation path

`learn.rs` is called from a command in the home window, never from
`session::deliver`. The hot path reads `vocabulary.json` and nothing else. A bug
in the learner cannot cost anyone a dictation, which is the
[governing rule](../CLAUDE.md#the-governing-rule) applied to a feature that
writes files.

---

## Undoing a fix, in the app you are typing in

A variant is a **whole-word replacement everywhere**, which is the point and also
the risk: `gently` → `ZentelAI` is right in "I need to go gently" and wrong in "I
need to act like gently". No filter can tell those apart, because the difference is
what the sentence means.

So there is **`Ctrl+Alt+Z`** (`Cmd+Alt+Z` on macOS): it rubs out what Piplo typed —
`SendInput` backspaces, the same synthesised input as typing — and types the version
with the terms put back. Press it again and the fix returns, so one chord is both
undo and redo.

**A shortcut and nothing else.** There was a widget-menu item too, and it was
removed: undoing a replacement is something you do while looking at the text, and
reaching for the widget means moving the mouse away from it — so the menu row was a
worse way to do the same thing, taking up a fifth of a four-item menu and
truncating the words it was trying to show. Fixed chord rather than a fifth
[setting](SETTINGS.md#other-shortcuts-listed-not-set) — it is listed there
read-only instead — and registered on key release so holding it cannot flip the text
back and forth. A chord another app already owns is shown as unavailable rather than
promised; the replacement still works, and the Vocabulary page can still remove the
rule.

**Independent of the [grammar undo](GRAMMAR.md#undoing-the-cleanup-after-the-fact),**
which is `Ctrl+Alt+G` and shares this machinery. Undoing the cleanup leaves a word
fix in force and vice versa: they are separate judgements, because the cleanup can
be wrong where the replacement was right.

**It refuses to type into Piplo.** Synthesised keystrokes go wherever focus is, so
if Piplo's own window is in front the undo would rub out part of the history list.
The guard compares only the owning process of the focused window — no window text,
nothing about anyone else's application — and says *click back into your document
first*.

- **Only the most recent dictation**, and only when a replacement actually fired.
  Any dictation that replaces nothing clears the offer, because the item rubs out a
  character count and the wrong count would eat text Piplo did not write.
- **It assumes the caret has not moved.** Piplo does not read the target
  application to check — that is the boundary this whole feature is built against,
  and the item is worded as an undo of the last thing typed precisely because that
  is all it can honestly promise.
- **It reverts the terms, not the sentence.** The reversal runs on the *final*
  text, so grammar's work survives and only the terms go back to what was heard.
- **A snippet expansion is never offered**, and neither is a dictation that was
  copied to the clipboard rather than typed.
- The rule itself is untouched. Removing it is the Vocabulary page's job — this is
  about the sentence in front of you.

### An idea, not built: what kind of word is this

The other way to fix the same problem: ask, when the term is added, what the word
*is* — an office name, a product, a person — and let the grammar model decide per
sentence whether the replacement belongs.

Recorded here because it is the only approach that could tell those two sentences
apart. Not built, and it would not replace the table: a model's judgement cannot be
the guarantee, and the deterministic replacement is what makes the feature
trustworthy. It would be a *softener* on top — one more field to fill in, one more
thing for the model to get wrong — so it needs a real design before it is worth
having.

---

## Storage

Next to `settings.json` and `snippets.json` in the app config dir, for the
[same reason](SETTINGS.md#where-they-live).

```
%APPDATA%\com.codea.piplo\
  settings.json
  snippets.json
  vocabulary.json      the terms and their variants — read by the pipeline
  corrections.jsonl    every correction event — read only by the learner
```

**`vocabulary.json`** — the live list.

```json
[
  {
    "id": "uuid-v4",
    "term": "ZentelAI",
    "variants": ["gentle AI", "zentel ai"],
    "source": "learned",
    "enabled": true,
    "priority": 0
  },
  {
    "id": "uuid-v4",
    "term": "Piplo",
    "variants": [],
    "source": "manual",
    "enabled": true,
    "priority": 1
  }
]
```

`source` is `manual` or `learned`, and it is on the entry, not the variant — a
manually-added term that later gains a learned variant is still a term the user
asked for. What the list shows per variant comes from the ledger.

`enabled` is `false` for an entry the user has switched off. It keeps its
variants and its place in the list, but takes no part in a dictation: no hint, no
replacement, and no mention in the grammar prompt. All three, or "off" means
something different depending on which half of the feature you are looking at.

`priority` is `1` for a starred term and `0` otherwise, and it only ever affects
[the order of the prompt hint](#order-once-the-cap-starts-biting).

Both fields default when absent, so a `vocabulary.json` written before they
existed loads unchanged. `enabled` defaults to **true** specifically — a bool
defaulting to `false` would silently switch off every dictionary already on disk.

Terms and variants are capped at **60 characters**. An entry is a name, not a
sentence: one pasted paragraph would otherwise consume most of the 180-character
prompt window and crowd out every other term.

**`corrections.jsonl`** — append-only, one line per event, like
[history](TRANSCRIBE.md#historyrs).

```json
{"at":"2026-08-13T09:14:02Z","from":"gentle AI","to":"ZentelAI","status":"observed"}
{"at":"2026-08-14T11:02:44Z","from":"verbal","to":"Vercel","status":"rejected"}
```

Counts are derived by reading the file: the number of `observed` lines for a
pair, unless a later `rejected` line supersedes them. Append-only means no
read-modify-write on a file the user might be looking at, and it makes the
feature's real behaviour greppable — the same argument as `corrected` and
`snippet` in history.

```bash
grep -c '"status":"observed"' corrections.jsonl
```

### Write, then adopt

Inherited from [snippets](SNIPPETS.md#write-then-adopt), unchanged: the in-memory
list is replaced only after the file write succeeds, so a failed write leaves
memory and disk agreeing on the last good state.

### Corrupt files load as empty

A `vocabulary.json` that will not parse loads as no entries; a
`corrections.jsonl` line that will not parse is skipped and the rest of the file
is read. Dictation works fine without either, and neither may ever prevent a
launch.

---

## The modules

Two, deliberately. The source plan's own architecture note asks for it, and they
have different failure consequences — one runs on every dictation, the other runs
when a user clicks a button.

### `vocabulary.rs`

| Item | Purpose |
| ---- | ------- |
| `Entry { id, term, variants, source }` | Empty `id` on the way in means "this is new" |
| `Store(Mutex<Vec<Entry>>)` | The live list, read by the pipeline. Managed state |
| `load(app)` | Read at startup, falling back to none |
| `apply(&[Entry], text)` | `(String, bool)` — the rules above. **Unit-tested** |
| `prompt(&[Entry])` | The capped comma list for Whisper, or `None` when empty |
| `terms_in(&[Entry], text)` | The terms present, for the grammar prompt line |
| `commit(app, list)` | Write, then adopt, then return the list |

`apply`, `prompt` and `terms_in` are pure functions over data — no `AppHandle`,
no I/O — which is what makes them worth testing.

### `learn.rs`

| Item | Purpose |
| ---- | ------- |
| `candidate(typed, edited)` | `Option<Mapping>` — trim, then the filter. **Unit-tested** |
| `looks_like_a_term(to, from, first_word)` | The four signals. **Unit-tested** |
| `record(app, typed, edited)` | Append the event, recount, promote at three |
| `region(typed, field)` | `Option<String>` — locate Piplo's insertion inside a document by exact word runs at each end. **Unit-tested** |
| `learn_from_field(app, typed, field)` | `region` → `candidate` → save the term alone. No ledger write, no variant |
| `suggestions(app)` | Pairs at count two that are not rejected |
| `reject(app, mapping)` | Append the rejection, drop the variant if learned |

`learn.rs` depends on `vocabulary.rs`. The only thing that depends on `learn.rs`
is its commands and one call in `session::catch_up`, which runs on a detached
blocking thread at the start of a recording — so a bug here still cannot cost
anyone a dictation.

### Validation, in Rust

The backend is the only place that decides what is valid. The form pre-checks the
same rules for instant feedback, but never authoritatively.

| Rejected | Message |
| -------- | ------- |
| Term empty after trimming | "Give the entry a word to type." |
| A variant equal to its own term | "'{variant}' is already the term." |
| A variant claimed by another entry | "'{variant}' already becomes '{other}'." |
| Term already in the dictionary | "'{term}' is already here." |

Compare **normalized** — lowercased and whitespace-collapsed — and exclude the
entry being edited from its own clash check, or saving an unchanged row fails.
Same rule as [snippets](SNIPPETS.md#validation-in-rust), same reason.

One **warning**, which does not block:

> "verbal" is an ordinary word — it will be replaced everywhere you say it.

Shown when a manually-typed variant is a single lowercase word with no digits and
no internal punctuation. `verbal` → `Vercel` is a real request and a real footgun,
and the user gets to make the call. Learned variants are held to the
[stricter test](#the-eligibility-filter) and never reach this case.

### Commands

Every mutation returns **the whole list**, so the page replaces its state outright
rather than patching it.

| Command | Returns | Used by |
| ------- | ------- | ------- |
| `list_vocabulary` | `Vec<Entry>` | The page, on mount |
| `save_term` | `Result<Vec<Entry>, String>` | Create and update, keyed on `id` |
| `delete_term` | `Result<Vec<Entry>, String>` | Row delete |
| `list_suggestions` | `Vec<Suggestion>` | The page, on mount |
| `accept_suggestion` | `Result<Vec<Entry>, String>` | *Add* on a suggestion |
| `reject_suggestion` | `Result<Vec<Suggestion>, String>` | *Never* on a suggestion |
| `record_correction` | `Result<Option<Learned>, String>` | The history row's save |
| `undo_learned_term` | `Result<(), String>` | *Undo* on the widget's read-back note |

`record_correction` returns what it learned, if anything, so the row can say so
immediately. That is [Feature 5's toast](#feedback), in the only place it belongs.

---

## The page

A fourth sidebar entry in the [home window](HOME.md), so `DesktopWindow.tsx`'s
page state becomes `"home" | "vocabulary" | "snippets" | "settings"`. Still no
router.

```
┌──────────────┬────────────────────────────────────────────────┐
│              │  Vocabulary                          [+ Add]   │
│  ◉  Piplo    │  Words Piplo should get right — 14 saved.       │
│              │                                                │
│  ⌂  Home     │  ✨ Suggested                                   │
│  ⌸  Vocabulary│ ┌──────────────────────────────────────────┐  │
│  ⧉  Snippets │  │ shadcn  ← "shad CN", twice   Add  Never  │  │
│  ⚙  Settings │  └──────────────────────────────────────────┘  │
│              │                                                │
│              │  [ Search              ]  [ All ▾ ] [Newest ▾] │
│              │  ┌──────────────────────────────────────────┐  │
│              │  │ ZentelAI                     ✨  ✎  🗑   │  │
│              │  │ gentle AI · zentel ai   learned from 3    │  │
│              │  ├──────────────────────────────────────────┤  │
│              │  │ Next.js                          ✎  🗑   │  │
│              │  │ next JS                          Manual   │  │
│  v0.1.0      │  └──────────────────────────────────────────┘  │
└──────────────┴────────────────────────────────────────────────┘
```

### Suggestions

Only rendered when there are any. A section that is usually an empty box teaches
the user to ignore that part of the screen.

Each row: the proposed term, the variant that produced it, how many times it has
been seen, and two actions. *Add* creates or extends the entry; *Never* writes the
rejection.

### A row

Two lines, and the second is the interesting one.

| Line | Content |
| ---- | ------- |
| 1 | The term, a sparkle when learned, a filled star when starred, and the actions |
| 2 | The variants as `·`-separated text, and the provenance |

Provenance is "Manual", "Learned from 3 corrections", or both when a manual term
has picked up learned variants. This is the answer to "why is this here?", and a
dictionary that cannot answer it is a dictionary users delete wholesale.

Same **fixed-column grid** as [snippets](SNIPPETS.md#row-alignment): actions
occupy their column whether or not they are visible, so hovering cannot reflow the
row.

#### Off

A switched-off row is **dimmed in place**, and line 2 is replaced with
`Off · not hinted, not replaced` — which outranks the variants, because the first
question about a dimmed row is why it is dimmed. It is not hidden, not moved, and
not sorted to the bottom: a term that vanishes when you turn it off has been
deleted as far as the user is concerned.

Its power button is also the one action that **stays visible without hovering**.
Hiding the only control that undoes the state, until you guess to hover for it, is
how a toggle becomes a delete.

#### Both toggles go through `save_term`

Star and on/off do not get commands of their own. They send the whole entry
through the same `save_term` that an edit uses, so the backend stays the only
thing that decides what is valid and a toggle cannot drift from what an edit would
have written. `priority` is clamped to the two bands on the way in — the value
crosses from the webview, and an out-of-range one would sort in a way no button
can undo.

### Editing

**In place, inside the row**, like a snippet. Term on one line, variants on
another as a comma-separated field — a chip editor for a list that is usually two
items long is more UI than the job needs.

- `Enter` saves from the term field, `Escape` cancels and `stopPropagation`s.
- The form keeps its own draft state; the saved list changes only when the
  backend accepts it.
- A rejected save is shown **inside the form**, next to the field that caused it.
- Typing clears the error.

### Deleting

Inline confirm in the row, pinned open while asking. Deleting a learned entry says
what else it does:

> Delete ZentelAI? Piplo will stop learning it too.

Because it will, and finding that out later is worse than reading it now.

### Search, filter, sort

All client-side over the loaded list, and the toolbar only appears above two
entries.

- **Search** matches the term **or any variant**, case-insensitively. Searching
  "gentle" must find `ZentelAI` — the whole point is finding the entry by the
  mistake, since that is what the user just saw.
- **Filter**: All · Manual · Learned. Three states, because "where did this come
  from" is the only axis worth filtering on.
- **Sort**: Newest · Oldest · A–Z, leaning on file order like
  [snippets](SNIPPETS.md#search-and-sort).

### Empty states

Two, and neither may be visible while still loading — track `loaded` explicitly.

**Nothing yet** explains the idea with three examples in the `variant → term`
shape, and says corrections are learned automatically. The examples live here and
nowhere else.

**Nothing matched** offers *Clear search*.

### Pagination

`Pagination.tsx` again, 20 rows, cursor clamped on shrink, reset to page 1 on
search, filter or sort. Same component, same rules, no third copy.

---

## Feedback

When something is learned, the confirmation appears **where the correction was
made** — inline in the history row that was just edited.

```
✨  Learned: ZentelAI   ·  from 3 corrections   ·  Undo   View
```

No toast system, no notification layer, no new window. The user is looking at
that row; anything else would be a mechanism built to tell someone something they
are already staring at.

*Undo* rejects the mapping and removes the variant. It stays until the row is
collapsed, then it is gone — the Vocabulary page is the place to change your mind
later.

---

## Settings

One new setting, making four.

| Setting | Default | Effect |
| ------- | ------- | ------ |
| **Learn from my corrections** | On | Whether editing a history row records a correction |

**Only the learning half has a switch.** The terms themselves do not need one:
they are words the user typed in, and a toggle that ignores what you typed is a
worse control than deleting the entry. Learning is different because it is the
only part that creates data on its own.

Off means `record_correction` returns `None` and writes nothing.
`corrections.jsonl` is untouched, existing entries keep working, and turning it
back on resumes from the counts already there.

---

## History logging

Two additions to the entry.

```json
{"…", "text": "I need to check ZentelAI.",
      "vocabulary": true,
      "edited": "I need to check ZentelAI, please."}
```

- **`vocabulary`** — the apply step changed something. Alongside `corrected` and
  `snippet`, for the same reason: the feature's real-world hit rate should be
  greppable rather than guessed at.
- **`edited`** — the user's correction, present only on rows they fixed. `text`
  keeps meaning **what was typed into the application**, which is the one
  guarantee that field has and which nothing here may change.

```bash
grep -c '"vocabulary":true' history/history.jsonl
grep -c '"edited"' history/history.jsonl
```

### Rewriting the file

Editing a row means rewriting `history.jsonl` whole. That is a change of character
for an append-only log, and it is the right trade: the file is
[already read whole](HOME.md#scale) on every render, so rewriting it is the same
cost class, and `clear_history` already truncates it.

The alternative — appending a supersede-record — pushes a fold onto every reader
of the file forever, to save a write that happens when a human clicks a button.

Write to a temporary file and rename over the original, so an interrupted write
cannot leave a truncated history.

---

## Checks

1. **A term reaches Whisper.** Add `ZentelAI`, dictate a sentence containing it,
   and confirm the request carries a `prompt`. With an empty dictionary, confirm
   the field is absent entirely.
2. **A variant fires.** With `gentle AI` → `ZentelAI` saved, force the mistake and
   watch the right text get typed.
3. **Whole words only.** A variant of `verbal` → `Vercel` must leave "verbally"
   alone. This one keeps the feature trustworthy in documents.
4. **Longest wins.** With both `gentle AI` and `gentle AI studio` mapped, the
   longer phrase produces the longer term, whatever order they are in the file.
5. **Grammar cannot undo it.** Make the cleanup model split the term — the second
   pass puts it back, and the typed text is correct.
6. **The candidate is extracted.** Edit a history row from "gentle AI" to
   "ZentelAI"; `corrections.jsonl` gains one `observed` line.
7. **Ordinary edits are not learned.** "yesterday" → "last night", "he go" → "he
   goes", "very good" → "excellent". None of them are recorded. This is the check
   the whole feature's credibility rests on.
8. **A wide edit is not learned.** Change two separate words in one row and
   confirm nothing is recorded.
9. **Three is the threshold.** The same correction once — silent. Twice — it
   appears under Suggested. Three times — it is a variant, and the row says so.
10. **Never means never.** Reject a suggestion, then make the same correction
    twice more. It does not come back.
11. **Undo removes and rejects.** Delete a learned variant, correct the same way
    again — it stays gone.
12. **A snippet expansion teaches nothing.** Edit the row of an expanded snippet
    and confirm no correction is recorded.
13. **The toggle is honest.** Turn learning off, edit a row, and confirm
    `corrections.jsonl` is byte-identical afterwards.
14. **A failed write changes nothing.** Make `vocabulary.json` read-only and save:
    an error, and the list is exactly as it was, including after a restart.
15. **Corrupt both files by hand** → the app starts, the page is empty, dictation
    still works, and the next save fixes them.
16. **Search finds the mistake.** Searching "gentle" returns `ZentelAI`.
17. **History survives an interrupted rewrite.** Kill the app mid-edit; the file
    is either the old version or the new one, never half of one.
18. **Off is off in all three places.** Switch a term off, then dictate its
    variant: the request carries no `prompt` naming it, the replacement does not
    fire, and the grammar prompt does not list it. Checking only the replacement
    is how this ships half-done.
19. **Off survives a restart, and the entry is intact.** The row is still there,
    still dimmed, with its variants — not deleted, not emptied.
20. **An old file loads switched on.** Take a `vocabulary.json` with no `enabled`
    or `priority` keys, start the app, and confirm every entry still fires. This
    is the migration, and getting it wrong silently disables the user's whole
    dictionary.
21. **A star reaches the front.** With more than 180 characters of terms, star one
    that was falling off the end and confirm it now leads the `prompt`.
22. **The cap never splits a term.** With a long dictionary, every term in the
    `prompt` is whole.
23. **60 characters is enforced.** Paste a sentence into the term field and into
    the variants field; both are rejected with a message, and nothing is written.

---

## What this is not

Each of these is in the source plan, and each is deliberately absent. The list is
longer than the feature because the feature is the part that survived.

- **No context awareness.** Not the active app, not the website, not the file open
  in the editor, and nothing read between dictations. The single read at the start
  of a recording is [the one exception](#reading-the-focused-field), it is
  anchored to Piplo's own last insertion, and it keeps nothing. This is still the
  single largest cut.
- **No keyboard hook.** Piplo does not watch what you type, here or
  [anywhere](SNIPPETS.md#what-this-is-not). The read-back does not change this: it
  looks at a field's value once, never at a keystroke.
- **No project scanning.** No `package.json`, no README, no source tree, no
  filenames. A dictation app that crawls your disk is a different product.
- **No fuzzy or phonetic matching.** [Why](#why-not-fuzzy).
- **No code formatting.** "get user by ID" stays "get user by ID". Producing
  `getUserById` requires knowing you are in an editor, which requires the context
  engine that is not being built.
- **No spoken self-correction.** "Deploy to Vercel… actually Netlify" types both.
  That is a speech feature, not a dictionary one, and it belongs to a different
  milestone if it ever exists.
- **No team dictionary, no sync, no import/export.** `vocabulary.json` is a plain
  file the user can copy. Accounts and sync are on the
  [do-not list](../CLAUDE.md#do-not-implement).
- **No confidence float.** [A count](#confidence-is-a-count).
- **No usage counts.** That is analytics.
- **No type taxonomy.** `person`, `brand`, `company`, `product`, `technology` —
  nine categories that change nothing about what the code does. A field the user
  must fill in and nothing ever reads is worse than no field.
- **No starring or pinning.** In a searchable list of a few dozen entries, it
  sorts nothing anyone was struggling to find.
