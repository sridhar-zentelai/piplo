# Self-corrections

People correct themselves mid-sentence. They say:

> "Let's meet on Monday, no wait, make that Tuesday."

and they mean:

> Let's meet on Tuesday.

Whisper transcribes the first one faithfully — correctly, that is its job — so
without this the correction lands in the document and has to be deleted by hand.
That hand-editing is the exact cost dictation exists to remove.

This is part of [grammar](GRAMMAR.md), not a step of its own: the same request
does the work, so it costs no extra latency and no extra call.

```
whisper → grammar (cleanup + corrections) → type into focused app
```

One rule defines it: **only obvious corrections.** When it is not clear what is
being retracted, nothing changes.

---

## The two shapes

Both are handled, and the second is the one that matters.

**Announced.** A phrase introduces the correction — "no wait", "I mean",
"scratch that", "actually".

> "Send it to Bob, sorry, I mean Rob." → "Send it to Rob."

**Restated.** Nothing announces it at all. The speaker simply says the phrase
again with a different word and expects the first attempt to vanish.

> "I wanted to buy a record as a gift, as a present." → "I wanted to buy a record as a present."

Restatement is plausibly the commoner of the two — most people just say it again
rather than announcing that they are about to. It is also invisible to any
implementation built on matching trigger phrases, which is why there is no list
of them anywhere in `grammar.rs`.

**The model decides what is a correction. Rust only checks the result is safe to
type.** The same distinction the model has to draw is the one a string match
cannot: "I actually enjoyed the film" contains "actually" and corrects nothing.

---

## Why the length guard had to change

`accept()` rejects any candidate shorter than `MIN_RATIO` (0.60) of the input.
That guard is what stops "what is the capital of France" being typed as "Paris",
and it is not negotiable.

But corrections delete a clause on purpose, and they land well underneath it:

| Raw | Result | Ratio |
|---|---|---|
| `Let's meet on Monday, no wait, make that Tuesday.` | `Let's meet on Tuesday.` | 0.45 |
| `Send it to Bob, sorry, I mean Rob.` | `Send it to Rob.` | 0.44 |
| `Let's do coffee at 2 actually 3.` | `Let's do coffee at 3.` | 0.66 |

So teaching the prompt about corrections and leaving the guard alone produces a
feature that appears to do nothing: the model does the right thing, `accept()`
discards it, and the raw transcript types. The floor had to open — but only where
a correction actually happened.

---

## What opens the floor

Two checks, both computed from the candidate's own words. Neither asks the model
what it did, because a model claiming "I applied a correction" would be opening
its own gate.

**Containment** — `contained_fraction`, at least `MIN_CONTAINED` (0.8).

The fraction of the candidate's words that the speaker actually said. A
correction only ever *removes* words, so what is left is drawn almost entirely
from the take. An invention is not:

| Raw | Candidate | Contained |
|---|---|---|
| `Let's meet on Monday, no wait, make that Tuesday.` | `Let's meet on Tuesday.` | 1.00 |
| `I wanted to buy a record as a gift, as a present.` | `I wanted to buy a record as a present.` | 1.00 |
| `what is the capital of France` | `Paris` | **0.00** |

Not 1.00 for ordinary cleanup — fixing a spelling or dropping in an article
introduces words that were never spoken — hence the slack in the threshold.

**The ending kept** — `keeps_the_ending`, within `TAIL_WINDOW` (4) words.

Containment alone would wave through a truncated response: the first twenty words
of a sixty-word take are all words that were said, so it scores 1.00 while
costing the user two-thirds of their dictation. A correction takes a bite out of
the middle and finishes on the speaker's final thought; a truncation stops early,
wherever the model ran out. Both are simply "shorter", so length cannot separate
them.

Only with **both** does the floor drop to `CORRECTION_MIN_RATIO` (0.25).
Everything else keeps 0.60. `MAX_RATIO` is untouched at 1.80 — a correction never
makes text longer.

---

## Watching it

The rejection line reports both signals alongside the ratio:

```
piplo: grammar rejected (length ratio 0.41, contained 0.93, ending kept)
```

A rejection that was **contained and kept its ending** is a correction the
thresholds are set fractionally too tight for — that is a number to tune. A
rejection with **low containment** is the guard doing its job and should be left
alone. The old log line could not tell those apart, and the difference is the
whole distance between tuning the thresholds and guessing at them.

---

## Known limits

- **A wrongly deleted clause is indistinguishable from a correctly deleted one.**
  "The deploy went out this morning and everything looks healthy so far." →
  "The deploy looks healthy so far." passes: contained, the speaker's own ending,
  shorter. Even the proportion removed matches a real correction almost exactly
  (58% against 56%), so no threshold separates them. Only the prompt can, which
  is why it is told to change nothing when the correction is not obvious. Pinned
  as `cannot_tell_an_over_eager_deletion_from_a_correction` so the limit stays
  visible. The raw transcript in the log is the backstop.
- **Nothing happens with grammar off.** `PIPLO_GRAMMAR=0` turns this off with it;
  corrections are post-processing, not capture.
- **The thresholds are heuristics.** 0.8 and 0.25 are first cuts chosen to clear
  the cases above with room to spare. The log line above is how they get checked
  against real dictation.
- **Not voice commands.** "Delete that", "new paragraph", "select all" as
  *commands* are a different feature with a different failure mode. This only
  handles the speaker correcting words in the same utterance.

---

## Tests

In `grammar.rs`, alongside the other `accept` tests. All pure — a fixed pair of
strings each, no network and no model:

- `accepts_a_spoken_self_correction` — the four shapes, including the restatement
  with no cue.
- `rejects_a_truncated_response` — containment 1.00, ending lost.
- `rejects_an_answer_instead_of_a_correction` — the "Paris" case, which now fails
  containment as well as length.
- `leaves_a_word_that_only_looks_like_a_correction` — "I actually enjoyed the
  film".
- `compares_words_without_punctuation_or_case`.
- `cannot_tell_an_over_eager_deletion_from_a_correction` — the known limit above.

Worth checking by hand as well, because no test says whether the model over-deletes
in practice: dictate a plain sentence with no correction and confirm it comes out
exactly as it did before, then a bare restatement in your own words, then "what is
the capital of France" — which must still type the question.
