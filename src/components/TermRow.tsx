import { useState } from "react";
import { Pencil, Sparkles, Trash2 } from "lucide-react";
import { Button } from "@/components/ui/button";
import type { Term } from "@/lib/commands";
import { cn } from "@/lib/utils";

/**
 * Two lines rather than the snippet row's one: a term and its variants are not a
 * key and a value, they are a word and the several ways it comes out wrong. The
 * actions cell reserves its width whether or not the buttons are visible, so
 * revealing them on hover cannot reflow the row.
 */
const GRID = "grid grid-cols-[minmax(0,1fr)_auto] items-start gap-3";

/** Comma-separated, because a chip editor for a list that is usually two items
 *  long is more UI than the job needs. */
const SEPARATOR = ", ";

export function joinVariants(variants: string[]): string {
  return variants.join(SEPARATOR);
}

function splitVariants(text: string): string[] {
  return text
    .split(",")
    .map((variant) => variant.trim())
    .filter((variant) => variant !== "");
}

export default function TermRow({
  entry,
  onEdit,
  onDelete,
}: {
  entry: Term;
  onEdit: () => void;
  onDelete: () => void;
}) {
  /** Pins the action column open, or it would vanish mid-question. */
  const [confirming, setConfirming] = useState(false);
  const learned = entry.source === "learned";

  return (
    <li className="group rounded-xl border border-border bg-white/[0.02] transition-colors hover:border-white/[0.14]">
      {/* Clicking anywhere on the row starts editing; the pencil is what says so.
          A div rather than a button: the actions are buttons of their own and
          nesting them would be invalid. */}
      <div
        role="button"
        tabIndex={0}
        aria-label={`Edit ${entry.term}`}
        onClick={onEdit}
        onKeyDown={(event) => {
          if (event.key === "Enter" || event.key === " ") {
            event.preventDefault();
            onEdit();
          }
        }}
        className={cn(
          GRID,
          "cursor-pointer rounded-xl p-3.5 outline-none focus-visible:border-ring",
        )}
      >
        <div className="min-w-0">
          <div className="flex items-center gap-1.5">
            <span
              title={entry.term}
              className="truncate font-mono text-xs text-foreground"
            >
              {entry.term}
            </span>
            {learned && (
              <Sparkles
                aria-label="Learned from your corrections"
                className="size-3 shrink-0 text-primary"
              />
            )}
          </div>

          {/* The answer to "why is this here?". A dictionary that cannot answer
              it is one users delete wholesale. */}
          <p className="mt-1 truncate font-sans text-xs text-muted-foreground">
            {entry.variants.length > 0 ? (
              <span title={entry.variants.join(" · ")}>
                {entry.variants.join(" · ")}
              </span>
            ) : (
              <span className="text-muted-foreground/50">
                Hinted to Whisper · learning the mishearings
              </span>
            )}
          </p>
        </div>

        <div
          className="flex min-w-[68px] items-center justify-end gap-1"
          // The row's own click starts editing, which is not what any of these
          // mean.
          onClick={(event) => event.stopPropagation()}
        >
          {confirming ? (
            <div className="flex items-center gap-1.5 whitespace-nowrap">
              <span className="font-sans text-xs text-muted-foreground">
                Delete?
              </span>
              <Button variant="destructive" size="xs" onClick={onDelete}>
                Delete
              </Button>
              <Button
                variant="ghost"
                size="xs"
                onClick={() => setConfirming(false)}
              >
                Cancel
              </Button>
            </div>
          ) : (
            <div className="flex items-center gap-1 opacity-0 transition-opacity focus-within:opacity-100 group-hover:opacity-100">
              <Button
                variant="ghost"
                size="icon-sm"
                aria-label="Edit term"
                onClick={onEdit}
              >
                <Pencil className="size-3.5" />
              </Button>
              <Button
                variant="ghost"
                size="icon-sm"
                aria-label="Delete term"
                onClick={() => setConfirming(true)}
              >
                <Trash2 className="size-3.5" />
              </Button>
            </div>
          )}
        </div>
      </div>
    </li>
  );
}

/** Which field a rejection belongs next to. */
type Field = "term" | "variants";

/**
 * The edit form, in place inside the row — not a dialog. A word and its
 * misspellings are not enough to justify taking over the window.
 *
 * **One field by default.** Asking the user which mishearings to expect is asking
 * them to guess what Whisper will do, which is the learner's job: variants arrive
 * from corrections. The replacement-rule field is a second, opt-in mode for the
 * user who already knows the mistake and wants it fixed now.
 *
 * It keeps its own draft: the saved list only changes once the backend accepts it.
 */
export function TermForm({
  entry,
  onSave,
  onCancel,
}: {
  entry: Term;
  /** Rejects with the backend's message, which is shown next to the field. */
  onSave: (entry: Term) => Promise<void>;
  onCancel: () => void;
}) {
  const [term, setTerm] = useState(entry.term);
  const [variants, setVariants] = useState(joinVariants(entry.variants));
  /** Open when the entry already has variants — hiding saved data behind a
   *  disclosure would make an edit look like a delete. */
  const [correcting, setCorrecting] = useState(entry.variants.length > 0);
  const [error, setError] = useState<{ field: Field; message: string } | null>(
    null,
  );
  const [busy, setBusy] = useState(false);

  const parsed = splitVariants(variants);
  /** A warning, not a rejection: `verbal` → `Vercel` is a real request and a real
   *  footgun, and the user gets to make the call. */
  const ordinary = parsed.filter(isOrdinaryWord);

  async function save() {
    if (busy) return;

    // Pre-checked here for instant feedback only. Rust is the authority, and it
    // checks the same things again.
    if (term.trim() === "") {
      setError({ field: "term", message: "Give the entry a word to type." });
      return;
    }

    setBusy(true);
    setError(null);

    try {
      await onSave({ ...entry, term: term.trim(), variants: parsed });
    } catch (cause) {
      const message = String(cause);
      // Only the term-already-here rejection is about the term itself; the other
      // two name a variant.
      setError({
        field: message.includes("already here") ? "term" : "variants",
        message,
      });
      setBusy(false);
    }
  }

  return (
    <li className="rounded-xl border border-white/[0.14] bg-white/[0.03] p-3.5">
      <div
        className="flex flex-col gap-2.5"
        onKeyDown={(event) => {
          if (event.key !== "Escape") return;
          // Or it also reaches whatever else listens for Escape.
          event.stopPropagation();
          onCancel();
        }}
      >
        <div>
          <label className="mb-1.5 block font-sans text-[11px] text-muted-foreground/70">
            Word or phrase — typed exactly as you write it
          </label>
          <input
            value={term}
            autoFocus
            spellCheck={false}
            autoComplete="off"
            placeholder="MongoDB"
            aria-label="Term"
            aria-invalid={error?.field === "term" || undefined}
            disabled={busy}
            // Typing clears it — the user is already answering it.
            onChange={(event) => {
              setTerm(event.target.value);
              setError(null);
            }}
            onKeyDown={(event) => {
              if (event.key === "Enter") {
                event.preventDefault();
                void save();
              }
            }}
            className={cn(
              "w-full rounded-lg border bg-white/[0.04] px-2.5 py-1.5 font-mono text-xs text-foreground outline-none placeholder:text-muted-foreground/50 focus-visible:border-ring",
              error?.field === "term" ? "border-destructive" : "border-input",
            )}
          />
          {error?.field === "term" && <FieldError>{error.message}</FieldError>}
        </div>

        {!correcting ? (
          <div className="flex items-baseline justify-between gap-3">
            <p className="font-sans text-[11px] leading-relaxed text-muted-foreground/70">
              Piplo will hint this word to Whisper, and learn the mishearings
              from the corrections you make.
            </p>
            <button
              type="button"
              onClick={() => setCorrecting(true)}
              className="shrink-0 font-sans text-[11px] text-muted-foreground underline decoration-dotted underline-offset-2 outline-none transition-colors hover:text-foreground focus-visible:text-foreground"
            >
              Correct a misspelling
            </button>
          </div>
        ) : (
          <div>
            <label className="mb-1.5 block font-sans text-[11px] text-muted-foreground/70">
              Replace these with it — what Piplo hears instead, separated by
              commas
            </label>
            <input
              value={variants}
              spellCheck={false}
              autoComplete="off"
              placeholder="mango DB, mongo db"
              aria-label="Variants"
              aria-invalid={error?.field === "variants" || undefined}
              disabled={busy}
              onChange={(event) => {
                setVariants(event.target.value);
                setError(null);
              }}
              onKeyDown={(event) => {
                if (event.key === "Enter") {
                  event.preventDefault();
                  void save();
                }
              }}
              className={cn(
                "w-full rounded-lg border bg-white/[0.04] px-2.5 py-1.5 font-mono text-xs text-foreground outline-none placeholder:text-muted-foreground/50 focus-visible:border-ring",
                error?.field === "variants"
                  ? "border-destructive"
                  : "border-input",
              )}
            />
            {error?.field === "variants" ? (
              <FieldError>{error.message}</FieldError>
            ) : (
              ordinary.length > 0 && (
                <p className="mt-1.5 font-sans text-xs text-muted-foreground">
                  “{ordinary[0]}” is an ordinary word — it will be replaced
                  everywhere you say it.
                </p>
              )
            )}
          </div>
        )}

        <div className="flex items-center justify-between gap-3">
          <p className="font-mono text-[11px] text-muted-foreground/50">
            Enter to save · Esc to cancel
          </p>
          <div className="flex items-center gap-2">
            <Button variant="ghost" size="sm" disabled={busy} onClick={onCancel}>
              Cancel
            </Button>
            <Button size="sm" disabled={busy} onClick={() => void save()}>
              Save
            </Button>
          </div>
        </div>
      </div>
    </li>
  );
}

/** A single lowercase word with no digits and no internal punctuation — the
 *  shape of a variant that will fire inside ordinary sentences. */
function isOrdinaryWord(variant: string): boolean {
  return /^[a-z]+$/.test(variant);
}

function FieldError({ children }: { children: React.ReactNode }) {
  return <p className="mt-1.5 font-sans text-xs text-destructive">{children}</p>;
}
