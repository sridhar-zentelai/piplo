import { useEffect, useState } from "react";
import { Trash2 } from "lucide-react";
import HistoryRow from "@/components/HistoryRow";
import MicIcon from "@/components/MicIcon";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogClose,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
  DialogTrigger,
} from "@/components/ui/dialog";
import { useHistory } from "@/hooks/useHistory";
import { clearHistory, getSettings } from "@/lib/commands";

export default function HomePage() {
  const { entries, loading, refresh } = useHistory();

  async function onClear() {
    try {
      await clearHistory();
      await refresh();
    } catch (error) {
      console.error("[piplo] could not clear history", error);
    }
  }

  return (
    <>
      <header className="flex items-center justify-between border-b border-border px-6 py-4">
        <h1 className="font-display text-lg font-semibold tracking-tight">
          History
        </h1>

        <div className="flex items-center gap-3">
          <span className="font-mono text-xs text-muted-foreground">
            {entries.length} {entries.length === 1 ? "dictation" : "dictations"}
          </span>

          {/* A shadcn dialog, never window.confirm — a native modal blocks the
              whole window and looks nothing like the app. */}
          <Dialog>
            <DialogTrigger asChild>
              <Button
                variant="ghost"
                size="icon"
                aria-label="Clear all"
                disabled={entries.length === 0}
              >
                <Trash2 className="size-4" />
              </Button>
            </DialogTrigger>
            <DialogContent>
              <DialogHeader>
                <DialogTitle>Clear all dictations?</DialogTitle>
                <DialogDescription>
                  This permanently deletes all {entries.length} entries from
                  history. It cannot be undone.
                </DialogDescription>
              </DialogHeader>
              <DialogFooter>
                <DialogClose asChild>
                  <Button variant="ghost">Cancel</Button>
                </DialogClose>
                <DialogClose asChild>
                  <Button variant="destructive" onClick={onClear}>
                    Clear all
                  </Button>
                </DialogClose>
              </DialogFooter>
            </DialogContent>
          </Dialog>
        </div>
      </header>

      <div className="min-h-0 flex-1 overflow-y-auto px-6 py-4">
        {loading ? null : entries.length === 0 ? (
          <EmptyState />
        ) : (
          <ul className="flex flex-col gap-2">
            {entries.map((entry) => (
              <HistoryRow key={entry.id} entry={entry} />
            ))}
          </ul>
        )}
      </div>
    </>
  );
}

/** The first thing a new user sees, so it says how to start. */
function EmptyState() {
  const [shortcut, setShortcut] = useState<string | null>(null);

  useEffect(() => {
    void getSettings()
      .then((settings) => setShortcut(settings.shortcut))
      .catch(() => setShortcut(null));
  }, []);

  return (
    <div className="flex h-full flex-col items-center justify-center gap-3 text-center">
      <MicIcon className="size-8 text-muted-foreground/40" />
      <p className="font-sans text-sm text-muted-foreground">
        No dictations yet
      </p>
      {shortcut && (
        <p className="font-sans text-xs text-muted-foreground/70">
          Hold{" "}
          <kbd className="rounded border border-border bg-white/[0.06] px-1.5 py-0.5 font-mono text-[11px] text-foreground">
            {shortcut}
          </kbd>{" "}
          and speak
        </p>
      )}
    </div>
  );
}
