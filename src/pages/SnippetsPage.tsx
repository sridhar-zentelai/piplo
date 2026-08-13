import { useEffect, useMemo, useRef, useState } from "react";
import { Plus, Replace, Search, SearchX } from "lucide-react";
import EmptyState from "@/components/EmptyState";
import Pagination from "@/components/Pagination";
import SnippetRow, { SnippetForm } from "@/components/SnippetRow";
import { Button } from "@/components/ui/button";
import {
  deleteSnippet,
  listSnippets,
  saveSnippet,
  type Snippet,
} from "@/lib/commands";
import { cn } from "@/lib/utils";

const PAGE_SIZE = 20;

/** Newest and Oldest lean on file order — snippets are appended as created, so
 *  the end of the list is the most recent. Cheaper than a timestamp field nobody
 *  would ever see. */
type Sort = "newest" | "oldest" | "az";

const SORTS: { id: Sort; label: string }[] = [
  { id: "newest", label: "Newest" },
  { id: "oldest", label: "Oldest" },
  { id: "az", label: "A–Z" },
];

/** Only on the nothing-yet state, and nowhere else — once there are real
 *  snippets, examples would be a second list of fake ones above the true one. */
const EXAMPLES: [string, string][] = [
  ["my email", "codeaprogram@gmail.com"],
  ["sign off", "Thanks,\nSridhar"],
  ["intro email", "Hi, I hope you're doing well."],
];

const BLANK: Snippet = { id: "", trigger: "", content: "" };

export default function SnippetsPage() {
  const [snippets, setSnippets] = useState<Snippet[]>([]);
  /** A page that has not answered yet looks exactly like a page with nothing on
   *  it, so neither empty state renders until the first response lands. */
  const [loaded, setLoaded] = useState(false);
  const [query, setQuery] = useState("");
  const [sort, setSort] = useState<Sort>("newest");
  const [page, setPage] = useState(1);
  const [creating, setCreating] = useState(false);
  const [editing, setEditing] = useState<string | null>(null);
  const scroller = useRef<HTMLDivElement>(null);

  useEffect(() => {
    void listSnippets()
      .then(setSnippets)
      .catch((error) => console.error("[piplo] could not read snippets", error))
      .finally(() => setLoaded(true));
  }, []);

  const matched = useMemo(() => {
    const needle = query.trim().toLowerCase();

    const found = needle
      ? snippets.filter(
          (snippet) =>
            snippet.trigger.toLowerCase().includes(needle) ||
            snippet.content.toLowerCase().includes(needle),
        )
      : snippets;

    switch (sort) {
      case "newest":
        return [...found].reverse();
      case "oldest":
        return found;
      case "az":
        return [...found].sort((a, b) =>
          a.trigger.localeCompare(b.trigger, undefined, { sensitivity: "base" }),
        );
    }
  }, [snippets, query, sort]);

  const pageCount = Math.max(1, Math.ceil(matched.length / PAGE_SIZE));

  // Deleting the last row on the last page must not leave the cursor pointing
  // past the end — here the count changes from inside the page.
  useEffect(() => {
    setPage((current) => Math.min(current, pageCount));
  }, [pageCount]);

  // Landing on page 3 of a fresh result set reads as a bug.
  useEffect(() => {
    setPage(1);
  }, [query, sort]);

  const visible = useMemo(
    () => matched.slice((page - 1) * PAGE_SIZE, page * PAGE_SIZE),
    [matched, page],
  );

  function goTo(next: number) {
    setPage(next);
    scroller.current?.scrollTo({ top: 0 });
  }

  /** Throws the backend's message, which the form shows next to the field. */
  async function save(snippet: Snippet) {
    setSnippets(await saveSnippet(snippet));
    setCreating(false);
    setEditing(null);
  }

  async function remove(id: string) {
    try {
      setSnippets(await deleteSnippet(id));
    } catch (error) {
      console.error("[piplo] could not delete the snippet", error);
    }
  }

  function startCreating() {
    setEditing(null);
    setCreating(true);
    // The form opens at the top of the list.
    goTo(1);
  }

  const searching = query.trim() !== "";
  // Nothing to search or sort below two.
  const showToolbar = snippets.length > 1;

  return (
    <>
      {/* The rule spans the window; the heading lines up with the content below. */}
      <header className="border-b border-border">
        <div className="mx-auto flex w-full max-w-3xl items-start justify-between gap-6 px-6 py-4">
          <div className="min-w-0">
            <h1 className="font-display text-lg font-semibold tracking-tight">
              Snippets
            </h1>
            <p className="mt-1 font-sans text-xs leading-relaxed text-muted-foreground">
              Say a trigger while dictating and Piplo types the text instead
              {loaded && snippets.length > 0 && ` — ${snippets.length} saved`}.
            </p>
          </div>

          <Button size="sm" onClick={startCreating} disabled={creating}>
            <Plus className="size-4" />
            New snippet
          </Button>
        </div>
      </header>

      <div
        ref={scroller}
        className="flex min-h-0 flex-1 flex-col overflow-y-auto"
      >
        <div className="mx-auto w-full max-w-3xl flex-1 px-6 pb-4">
          {showToolbar && (
            <div className="flex items-center gap-2 py-4">
              <div className="relative flex-1">
                <Search className="pointer-events-none absolute left-2.5 top-1/2 size-3.5 -translate-y-1/2 text-muted-foreground/50" />
                <input
                  value={query}
                  spellCheck={false}
                  placeholder="Search snippets"
                  aria-label="Search snippets"
                  onChange={(event) => setQuery(event.target.value)}
                  onKeyDown={(event) => {
                    if (event.key === "Escape") {
                      event.stopPropagation();
                      setQuery("");
                    }
                  }}
                  className="w-full rounded-lg border border-input bg-white/[0.04] py-1.5 pl-8 pr-2.5 font-sans text-sm text-foreground outline-none placeholder:text-muted-foreground/50 focus-visible:border-ring"
                />
              </div>

              {/* A segmented control rather than a <select>: WebView2 draws the
                  native listbox light however the page is themed, and Piplo is
                  dark only. Three options do not need a popup anyway. */}
              <div
                role="group"
                aria-label="Sort snippets"
                className="flex shrink-0 items-center gap-0.5 rounded-lg border border-input bg-white/[0.04] p-0.5"
              >
                {SORTS.map(({ id, label }) => (
                  <button
                    key={id}
                    type="button"
                    onClick={() => setSort(id)}
                    aria-pressed={sort === id}
                    className={cn(
                      "rounded-md px-2 py-1 font-sans text-xs transition-colors outline-none focus-visible:text-foreground",
                      sort === id
                        ? "bg-white/[0.10] text-foreground"
                        : "text-muted-foreground hover:text-foreground",
                    )}
                  >
                    {label}
                  </button>
                ))}
              </div>

              {searching && (
                <span className="shrink-0 font-mono text-[11px] text-muted-foreground">
                  {matched.length} of {snippets.length}
                </span>
              )}
            </div>
          )}

          {/* The toolbar already provides the gap under the header when it is
              there. */}
          <ul className={cn("flex flex-col gap-2", !showToolbar && "pt-4")}>
            {creating && (
              <SnippetForm
                snippet={BLANK}
                onSave={save}
                onCancel={() => setCreating(false)}
              />
            )}

            {visible.map((snippet) =>
              snippet.id === editing ? (
                <SnippetForm
                  key={snippet.id}
                  snippet={snippet}
                  onSave={save}
                  onCancel={() => setEditing(null)}
                />
              ) : (
                <SnippetRow
                  key={snippet.id}
                  snippet={snippet}
                  onEdit={() => {
                    setCreating(false);
                    setEditing(snippet.id);
                  }}
                  onDelete={() => void remove(snippet.id)}
                />
              ),
            )}
          </ul>

          {loaded && !creating && matched.length === 0 && (
            searching ? (
              <EmptyState
                icon={SearchX}
                title={`Nothing matches “${query.trim()}”`}
                hint="Search looks at both the trigger and the text it types."
              >
                <Button variant="outline" size="sm" onClick={() => setQuery("")}>
                  Clear search
                </Button>
              </EmptyState>
            ) : (
              <NothingYet onCreate={startCreating} />
            )
          )}
        </div>

        {matched.length > PAGE_SIZE && (
          <div className="sticky bottom-0 mx-auto w-full max-w-3xl bg-[#141414]">
            <Pagination
              page={page}
              pageCount={pageCount}
              from={(page - 1) * PAGE_SIZE + 1}
              to={Math.min(page * PAGE_SIZE, matched.length)}
              total={matched.length}
              onChange={goTo}
            />
          </div>
        )}
      </div>
    </>
  );
}

/** A user with an empty list does not know what a snippet is, so this explains
 *  the idea rather than just reporting the count. */
function NothingYet({ onCreate }: { onCreate: () => void }) {
  return (
    <EmptyState
      icon={Replace}
      title="No snippets yet"
      hint="Save a short phrase and the text it should become. Say the phrase on its own while dictating and Piplo types the text instead."
    >
      <ul className="mt-1 flex flex-col gap-1.5">
        {EXAMPLES.map(([trigger, content]) => (
          <li
            key={trigger}
            className="flex items-center gap-2 font-mono text-[11px] text-muted-foreground/60"
          >
            <span>“{trigger}”</span>
            <span aria-hidden>→</span>
            <span className="truncate">{content.replace(/\s+/g, " ")}</span>
          </li>
        ))}
      </ul>

      <Button variant="outline" size="sm" className="mt-2" onClick={onCreate}>
        <Plus className="size-4" />
        New snippet
      </Button>
    </EmptyState>
  );
}
