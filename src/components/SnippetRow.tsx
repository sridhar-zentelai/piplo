import { useState } from "react";
import { ArrowRight, Pencil, Trash2 } from "lucide-react";
import { Button } from "@/components/ui/button";
import type { Snippet } from "@/lib/commands";
import { IS_MAC } from "@/lib/shortcuts";
import { cn } from "@/lib/utils";

/**
 * The same three columns on every row, so nothing shifts between them. The
 * actions cell reserves its width whether or not the buttons are visible —
 * revealing them on hover must not reflow the row.
 */
const GRID = "grid grid-cols-[minmax(0,180px)_14px_minmax(0,1fr)_auto] items-center gap-3";

export default function SnippetRow({
  snippet,
  onEdit,
  onDelete,
}: {
  snippet: Snippet;
  onEdit: () => void;
  onDelete: () => void;
}) {
  /** Pins the action column open, or it would vanish mid-question. */
  const [confirming, setConfirming] = useState(false);

  return (
    <li className="group rounded-xl border border-border bg-white/[0.02] transition-colors hover:border-white/[0.14]">
      {/* Clicking anywhere on the row starts editing; the pencil is what says so.
          A div rather than a button: the actions are buttons of their own and
          nesting them would be invalid. */}
      <div
        role="button"
        tabIndex={0}
        aria-label={`Edit ${snippet.trigger}`}
        onClick={onEdit}
        onKeyDown={(event) => {
          if (event.key === "Enter" || event.key === " ") {
            event.preventDefault();
            onEdit();
          }
        }}
        className={cn(GRID, "cursor-pointer rounded-xl p-3.5 outline-none focus-visible:border-ring")}
      >
        <span
          title={snippet.trigger}
          className="truncate font-mono text-xs text-foreground"
        >
          {snippet.trigger}
        </span>

        <ArrowRight aria-hidden className="size-3.5 text-muted-foreground/40" />

        {/* One line, whatever the content does. Newlines would make rows
            different heights for no information the list needs. */}
        <span
          title={snippet.content}
          className="truncate font-sans text-sm text-muted-foreground"
        >
          {snippet.content.replace(/\s+/g, " ")}
        </span>

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
                aria-label="Edit snippet"
                onClick={onEdit}
              >
                <Pencil className="size-3.5" />
              </Button>
              <Button
                variant="ghost"
                size="icon-sm"
                aria-label="Delete snippet"
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
type Field = "trigger" | "content";

/**
 * The edit form, in place inside the row — not a dialog. A trigger and a body are
 * not enough to justify taking over the window.
 *
 * It keeps its own draft: a half-typed snippet is nobody else's business, and the
 * saved list only changes once the backend accepts it.
 */
export function SnippetForm({
  snippet,
  onSave,
  onCancel,
}: {
  snippet: Snippet;
  /** Rejects with the backend's message, which is shown next to the field. */
  onSave: (snippet: Snippet) => Promise<void>;
  onCancel: () => void;
}) {
  const [trigger, setTrigger] = useState(snippet.trigger);
  const [content, setContent] = useState(snippet.content);
  const [error, setError] = useState<{ field: Field; message: string } | null>(
    null,
  );
  const [busy, setBusy] = useState(false);

  async function save() {
    if (busy) return;

    // Pre-checked here for instant feedback only. Rust is the authority, and it
    // checks the same two things again.
    if (trigger.trim() === "") {
      setError({ field: "trigger", message: "Give the snippet something to say." });
      return;
    }

    if (content.trim() === "") {
      setError({ field: "content", message: "Give the snippet some text to type." });
      return;
    }

    setBusy(true);
    setError(null);

    try {
      await onSave({ ...snippet, trigger: trigger.trim(), content: content.trim() });
    } catch (cause) {
      const message = String(cause);
      // Only one of the three rejections is about the content, so that is the
      // one worth naming — anything else belongs beside the trigger.
      setError({
        field: message.includes("to type") ? "content" : "trigger",
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
          <input
            value={trigger}
            autoFocus
            spellCheck={false}
            autoComplete="off"
            placeholder="my email"
            aria-label="Trigger"
            aria-invalid={error?.field === "trigger" || undefined}
            disabled={busy}
            // Typing clears it — the user is already answering it.
            onChange={(event) => {
              setTrigger(event.target.value);
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
              error?.field === "trigger" ? "border-destructive" : "border-input",
            )}
          />
          {error?.field === "trigger" && <FieldError>{error.message}</FieldError>}
        </div>

        <div>
          <textarea
            value={content}
            rows={3}
            spellCheck={false}
            placeholder="codeaprogram@gmail.com"
            aria-label="Text to type"
            aria-invalid={error?.field === "content" || undefined}
            disabled={busy}
            onChange={(event) => {
              setContent(event.target.value);
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
            className={cn(
              "w-full resize-y rounded-lg border bg-white/[0.04] px-2.5 py-1.5 font-sans text-sm leading-relaxed text-foreground outline-none placeholder:text-muted-foreground/50 focus-visible:border-ring",
              error?.field === "content" ? "border-destructive" : "border-input",
            )}
          />
          {error?.field === "content" && <FieldError>{error.message}</FieldError>}
        </div>

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

function FieldError({ children }: { children: React.ReactNode }) {
  return (
    <p className="mt-1.5 font-sans text-xs text-destructive">{children}</p>
  );
}
