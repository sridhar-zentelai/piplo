import { useEffect, useMemo, useRef, useState } from "react";
import { Trash2 } from "lucide-react";
import EmptyState from "@/components/EmptyState";
import HistoryRow from "@/components/HistoryRow";
import HomeHero from "@/components/HomeHero";
import MicIcon from "@/components/MicIcon";
import Pagination from "@/components/Pagination";
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
import { clearHistory } from "@/lib/commands";

const PAGE_SIZE = 10;

export default function HomePage() {
  const { entries, loading, refresh } = useHistory();
  const [page, setPage] = useState(1);
  const scroller = useRef<HTMLDivElement>(null);

  const pageCount = Math.max(1, Math.ceil(entries.length / PAGE_SIZE));

  // Clearing history, or a re-fetch that shrank the list, can leave the cursor
  // past the end.
  useEffect(() => {
    setPage((current) => Math.min(current, pageCount));
  }, [pageCount]);

  const visible = useMemo(
    () => entries.slice((page - 1) * PAGE_SIZE, page * PAGE_SIZE),
    [entries, page],
  );

  function goTo(next: number) {
    setPage(next);
    // Page two starting halfway down the list reads as a broken jump.
    scroller.current?.scrollTo({ top: 0 });
  }

  async function onClear() {
    try {
      await clearHistory();
      await refresh();
    } catch (error) {
      console.error("[piplo] could not clear history", error);
    }
  }

  return (
    <div ref={scroller} className="flex min-h-0 flex-1 flex-col overflow-y-auto">
      <HomeHero entries={entries} />

      <header className="mx-auto flex w-full max-w-3xl items-center justify-between px-6 pb-2 pt-5">
        <h2 className="font-display text-sm font-semibold uppercase tracking-widest text-muted-foreground">
          History
        </h2>

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

      <div className="mx-auto w-full max-w-3xl flex-1 px-6 pb-4">
        {loading ? null : entries.length === 0 ? (
          <EmptyState
            icon={MicIcon}
            title="No dictations yet"
            hint="Hold your shortcut and speak — the text lands wherever your caret is."
          />
        ) : (
          <ul className="flex flex-col gap-2">
            {visible.map((entry) => (
              <HistoryRow key={entry.id} entry={entry} />
            ))}
          </ul>
        )}
      </div>

      {entries.length > PAGE_SIZE && (
        <div className="sticky bottom-0 mx-auto w-full max-w-3xl bg-[#141414]">
          <Pagination
            page={page}
            pageCount={pageCount}
            from={(page - 1) * PAGE_SIZE + 1}
            to={Math.min(page * PAGE_SIZE, entries.length)}
            total={entries.length}
            onChange={goTo}
          />
        </div>
      )}
    </div>
  );
}

