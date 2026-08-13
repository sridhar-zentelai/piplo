import { useState } from "react";
import { Check, Copy, Pencil, Sparkles } from "lucide-react";
import { writeText } from "@tauri-apps/plugin-clipboard-manager";
import { Button } from "@/components/ui/button";
import {
  acceptSuggestion,
  recordCorrection,
  rejectSuggestion,
  type HistoryEntry,
  type Outcome,
} from "@/lib/commands";
import { IS_MAC } from "@/lib/shortcuts";
import { cn } from "@/lib/utils";

export default function HistoryRow({
  entry,
  onCorrected,
}: {
  entry: HistoryEntry;
  /** Re-fetch, so the row shows the corrected text rather than the old one. */
  onCorrected: () => void;
}) {
  const [expanded, setExpanded] = useState(false);
  const [copied, setCopied] = useState(false);
  const [editing, setEditing] = useState(false);
  /** Survives the form closing: the note belongs where the correction was made,
   *  and it stays until the row is collapsed. */
  const [outcome, setOutcome] = useState<Outcome | null>(null);

  /** `text` is what was typed into the application and never changes. A row the
   *  user has fixed shows their version — that is the copy they came back for. */
  const shown = entry.edited ?? entry.text;

  async function copy() {
    try {
      // The plugin, not navigator.clipboard: the packaged app serves
      // http://tauri.localhost, which is not a secure context, so the browser API
      // is unavailable — and it works under `tauri dev`, so getting this wrong
      // breaks only in the build.
      await writeText(shown);
      setCopied(true);
      setTimeout(() => setCopied(false), 1400);
    } catch (error) {
      console.error("[piplo] copy failed", error);
    }
  }

  if (editing) {
    return (
      <CorrectionForm
        entry={entry}
        initial={shown}
        onCancel={() => setEditing(false)}
        onSaved={(result) => {
          setEditing(false);
          setOutcome(result);
          onCorrected();
        }}
      />
    );
  }

  return (
    <li className="group rounded-xl border border-border bg-white/[0.02] p-3.5 transition-colors hover:border-white/[0.14]">
      <div className="flex items-start gap-3">
        <button
          type="button"
          onClick={() => {
            setExpanded((open) => !open);
            // Collapsing is the gesture that dismisses the note — the Vocabulary
            // page is where you change your mind later.
            setOutcome(null);
          }}
          className="min-w-0 flex-1 text-left"
          aria-expanded={expanded}
        >
          {/* Click to expand in place — no modal, no detail page. */}
          <p
            className={cn(
              "font-sans text-sm leading-relaxed text-foreground",
              !expanded && "line-clamp-3",
            )}
          >
            {shown}
          </p>
        </button>

        <div className="flex shrink-0 items-center gap-0.5">
          {/* Labelled "fix it here" in the hint below, because fixing it in Piplo
              does not fix it in the document — the honest version of this
              affordance says what it is for. */}
          <button
            type="button"
            onClick={() => setEditing(true)}
            aria-label="Fix this dictation"
            title="Fix it here"
            className="rounded-md p-1.5 text-muted-foreground opacity-0 transition-all hover:bg-white/[0.06] hover:text-foreground focus-visible:opacity-100 group-hover:opacity-100"
          >
            <Pencil className="size-4" />
          </button>

          <button
            type="button"
            onClick={copy}
            aria-label={copied ? "Copied" : "Copy"}
            className="rounded-md p-1.5 text-muted-foreground opacity-0 transition-all hover:bg-white/[0.06] hover:text-foreground focus-visible:opacity-100 group-hover:opacity-100"
          >
            {copied ? (
              <Check className="size-4 text-primary" />
            ) : (
              <Copy className="size-4" />
            )}
          </button>
        </div>
      </div>

      <p className="mt-2 font-mono text-[11px] text-muted-foreground/70">
        {relativeTime(entry.at)}
        {" · "}
        {entry.duration_secs.toFixed(1)}s
        {entry.language ? ` · ${entry.language}` : ""}
        {entry.edited ? " · fixed" : ""}
      </p>

      {outcome && (
        <OutcomeNote outcome={outcome} onDone={() => setOutcome(null)} />
      )}
    </li>
  );
}

/**
 * What the correction did, inline in the row that was just edited. No toast
 * system, no notification layer — the user is looking at this row, and anything
 * else would be a mechanism built to tell someone something they are already
 * staring at.
 *
 * Two of the four outcomes are silent on purpose: a first or second sighting is
 * not news, and neither is an edit that was never a term.
 */
function OutcomeNote({
  outcome,
  onDone,
}: {
  outcome: Outcome;
  onDone: () => void;
}) {
  const [busy, setBusy] = useState(false);

  if (outcome.kind === "nothing" || outcome.kind === "counted") return null;

  async function act(run: () => Promise<unknown>) {
    setBusy(true);
    try {
      await run();
      onDone();
    } catch (error) {
      console.error("[piplo] could not change the learned variant", error);
      setBusy(false);
    }
  }

  if (outcome.kind === "refused") {
    /* Said out loud, because the alternative is a correction that silently does
       nothing forever and a user who cannot tell why. */
    return (
      <div className="mt-2.5 flex flex-wrap items-center gap-2 rounded-lg border border-border bg-white/[0.03] px-2.5 py-2">
        <p className="min-w-0 font-sans text-xs text-muted-foreground">
          Not learned — you removed{" "}
          <span className="font-mono text-foreground">{outcome.from}</span>{" "}
          before.
        </p>
        <Button
          variant="ghost"
          size="xs"
          className="ml-auto"
          disabled={busy}
          onClick={() =>
            void act(() =>
              acceptSuggestion({ from: outcome.from, to: outcome.to }),
            )
          }
        >
          Learn it anyway
        </Button>
      </div>
    );
  }

  return (
    <div className="mt-2.5 flex flex-wrap items-center gap-2 rounded-lg border border-primary/25 bg-primary/[0.07] px-2.5 py-2">
      <Sparkles aria-hidden className="size-3.5 shrink-0 text-primary" />
      <p className="font-sans text-xs text-foreground">
        Learned: <span className="font-mono">{outcome.term}</span>
      </p>
      <span className="font-mono text-[11px] text-muted-foreground">
        from {outcome.count} corrections
      </span>
      <Button
        variant="ghost"
        size="xs"
        className="ml-auto"
        disabled={busy}
        onClick={() =>
          // Rejects the mapping as well as removing the variant, so the next
          // correction does not bring it straight back.
          void act(() =>
            rejectSuggestion({ from: outcome.from, to: outcome.term }),
          )
        }
      >
        Undo
      </Button>
    </div>
  );
}

/**
 * Editing in place, like every other form in Piplo.
 *
 * Its own draft state, and the list only changes once Rust has rewritten the file.
 */
function CorrectionForm({
  entry,
  initial,
  onSaved,
  onCancel,
}: {
  entry: HistoryEntry;
  initial: string;
  onSaved: (outcome: Outcome) => void;
  onCancel: () => void;
}) {
  const [draft, setDraft] = useState(initial);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  async function save() {
    if (busy) return;

    const edited = draft.trim();

    if (edited === "" || edited === initial) {
      onCancel();
      return;
    }

    setBusy(true);
    setError(null);

    try {
      onSaved(await recordCorrection(entry.id, edited));
    } catch (cause) {
      setError(String(cause));
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
        <textarea
          value={draft}
          rows={3}
          autoFocus
          spellCheck={false}
          aria-label="Corrected text"
          disabled={busy}
          onChange={(event) => {
            setDraft(event.target.value);
            setError(null);
          }}
          onKeyDown={(event) => {
            // A textarea keeps Enter for newlines, so saving needs the modifier
            // every editor already uses.
            if (event.key === "Enter" && (event.ctrlKey || event.metaKey)) {
              event.preventDefault();
              void save();
            }
          }}
          className="w-full resize-y rounded-lg border border-input bg-white/[0.04] px-2.5 py-1.5 font-sans text-sm leading-relaxed text-foreground outline-none focus-visible:border-ring"
        />

        {error ? (
          <p className="font-sans text-xs text-destructive">{error}</p>
        ) : (
          /* Said out loud, because it is the honest trade: the text is already in
             the user's document, so this fixes the copy here and teaches Piplo. */
          <p className="font-sans text-xs leading-relaxed text-muted-foreground">
            Fix it here and Piplo stops making the mistake. Your document is not
            changed.
          </p>
        )}

        <div className="flex items-center justify-between gap-3">
          <p className="font-mono text-[11px] text-muted-foreground/50">
            {IS_MAC ? "Cmd" : "Ctrl"}+Enter to save · Esc to cancel
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

/** Computed on render from `at`, absolute past a week. */
function relativeTime(iso: string): string {
  const then = new Date(iso);

  if (Number.isNaN(then.getTime())) return "unknown time";

  const seconds = Math.round((Date.now() - then.getTime()) / 1000);

  if (seconds < 45) return "just now";
  if (seconds < 90) return "a minute ago";

  const minutes = Math.round(seconds / 60);
  if (minutes < 60) return `${minutes} minutes ago`;

  const hours = Math.round(minutes / 60);
  if (hours < 24) return hours === 1 ? "an hour ago" : `${hours} hours ago`;

  const days = Math.round(hours / 24);
  if (days === 1) return "yesterday";
  if (days < 7) return `${days} days ago`;

  return then.toLocaleDateString(undefined, {
    year: "numeric",
    month: "short",
    day: "numeric",
  });
}
