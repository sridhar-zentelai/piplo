import { useEffect, useMemo, useRef, useState } from "react";
import { ArrowRight, BookMarked, Plus, Search, SearchX, Sparkles } from "lucide-react";
import EmptyState from "@/components/EmptyState";
import Pagination from "@/components/Pagination";
import TermRow, { TermForm } from "@/components/TermRow";
import { Button } from "@/components/ui/button";
import {
  acceptSuggestion,
  deleteTerm,
  listSuggestions,
  listVocabulary,
  rejectSuggestion,
  saveTerm,
  type Suggestion,
  type Term,
} from "@/lib/commands";
import { cn } from "@/lib/utils";

const PAGE_SIZE = 20;

/** Newest and Oldest lean on file order — entries are appended as created, so the
 *  end of the list is the most recent. Same trade as the snippets page. */
type Sort = "newest" | "oldest" | "az";

const SORTS: { id: Sort; label: string }[] = [
  { id: "newest", label: "Newest" },
  { id: "oldest", label: "Oldest" },
  { id: "az", label: "A–Z" },
];

/** "Where did this come from" is the only axis worth filtering on. */
type Filter = "all" | "manual" | "learned";

const FILTERS: { id: Filter; label: string }[] = [
  { id: "all", label: "All" },
  { id: "manual", label: "Manual" },
  { id: "learned", label: "Learned" },
];

/** Only on the nothing-yet state. In the `variant → term` shape, because that is
 *  the shape of the thing being explained. */
const EXAMPLES: [string, string][] = [
  ["mango DB", "MongoDB"],
  ["next JS", "Next.js"],
  ["prism", "Prisma"],
];

const BLANK: Term = { id: "", term: "", variants: [], source: "manual" };

export default function VocabularyPage() {
  const [entries, setEntries] = useState<Term[]>([]);
  /** A page that has not answered yet looks exactly like a page with nothing on
   *  it, so neither empty state renders until the first response lands. */
  const [loaded, setLoaded] = useState(false);
  const [suggestions, setSuggestions] = useState<Suggestion[]>([]);
  const [query, setQuery] = useState("");
  const [sort, setSort] = useState<Sort>("newest");
  const [filter, setFilter] = useState<Filter>("all");
  const [page, setPage] = useState(1);
  const [creating, setCreating] = useState(false);
  const [editing, setEditing] = useState<string | null>(null);
  const scroller = useRef<HTMLDivElement>(null);

  useEffect(() => {
    void listVocabulary()
      .then(setEntries)
      .catch((error) =>
        console.error("[piplo] could not read the vocabulary", error),
      )
      .finally(() => setLoaded(true));

    void refreshSuggestions();
  }, []);

  async function refreshSuggestions() {
    try {
      setSuggestions(await listSuggestions());
    } catch (error) {
      console.error("[piplo] could not read the suggestions", error);
    }
  }

  const matched = useMemo(() => {
    const needle = query.trim().toLowerCase();

    const kept =
      filter === "all"
        ? entries
        : entries.filter((entry) => entry.source === filter);

    // The variants matter more than the term here: the user just saw the
    // mistake, so searching "mango" has to find MongoDB.
    const found = needle
      ? kept.filter(
          (entry) =>
            entry.term.toLowerCase().includes(needle) ||
            entry.variants.some((variant) =>
              variant.toLowerCase().includes(needle),
            ),
        )
      : kept;

    switch (sort) {
      case "newest":
        return [...found].reverse();
      case "oldest":
        return found;
      case "az":
        return [...found].sort((a, b) =>
          a.term.localeCompare(b.term, undefined, { sensitivity: "base" }),
        );
    }
  }, [entries, query, sort, filter]);

  const pageCount = Math.max(1, Math.ceil(matched.length / PAGE_SIZE));

  // Deleting the last row on the last page must not leave the cursor pointing
  // past the end — here the count changes from inside the page.
  useEffect(() => {
    setPage((current) => Math.min(current, pageCount));
  }, [pageCount]);

  // Landing on page 3 of a fresh result set reads as a bug.
  useEffect(() => {
    setPage(1);
  }, [query, sort, filter]);

  const visible = useMemo(
    () => matched.slice((page - 1) * PAGE_SIZE, page * PAGE_SIZE),
    [matched, page],
  );

  function goTo(next: number) {
    setPage(next);
    scroller.current?.scrollTo({ top: 0 });
  }

  /** Throws the backend's message, which the form shows next to the field. */
  async function save(entry: Term) {
    setEntries(await saveTerm(entry));
    setCreating(false);
    setEditing(null);
  }

  async function remove(id: string) {
    try {
      setEntries(await deleteTerm(id));
    } catch (error) {
      console.error("[piplo] could not delete the term", error);
    }
  }

  /** *Add* — the entry gains the variant, and the pair leaves the list because it
   *  is now in the dictionary. */
  async function accept(suggestion: Suggestion) {
    try {
      setEntries(await acceptSuggestion({ from: suggestion.from, to: suggestion.to }));
      await refreshSuggestions();
    } catch (error) {
      console.error("[piplo] could not accept the suggestion", error);
    }
  }

  /** *Delete* — forgets the suggestion. Not a block: if Piplo keeps hearing it the
   *  same way, it comes back. */
  async function forget(suggestion: Suggestion) {
    try {
      setSuggestions(
        await rejectSuggestion({ from: suggestion.from, to: suggestion.to }),
      );
    } catch (error) {
      console.error("[piplo] could not reject the suggestion", error);
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
  const showToolbar = entries.length > 1;

  return (
    <>
      {/* The rule spans the window; the heading lines up with the content below. */}
      <header className="border-b border-border">
        <div className="mx-auto flex w-full max-w-3xl items-start justify-between gap-6 px-6 py-4">
          <div className="min-w-0">
            <h1 className="font-display text-lg font-semibold tracking-tight">
              Vocabulary
            </h1>
            <p className="mt-1 font-sans text-xs leading-relaxed text-muted-foreground">
              Words Piplo should get right. Add the word; the mishearings are
              learned from your corrections
              {loaded && entries.length > 0 && ` — ${entries.length} saved`}.
            </p>
          </div>

          <Button size="sm" onClick={startCreating} disabled={creating}>
            <Plus className="size-4" />
            Add word
          </Button>
        </div>
      </header>

      <div
        ref={scroller}
        className="flex min-h-0 flex-1 flex-col overflow-y-auto"
      >
        <div className="mx-auto w-full max-w-3xl flex-1 px-6 pb-4">
          {/* Only rendered when there are any: a section that is usually an empty
              box teaches the user to ignore that part of the screen. */}
          {suggestions.length > 0 && (
            <section className="pt-4">
              <h2 className="mb-2 flex items-center gap-1.5 font-sans text-xs text-muted-foreground">
                <Sparkles aria-hidden className="size-3.5 text-primary" />
                Suggested from your corrections
              </h2>
              <ul className="flex flex-col gap-2">
                {suggestions.map((suggestion) => (
                  <li
                    key={`${suggestion.from}→${suggestion.to}`}
                    className="flex items-center gap-3 rounded-xl border border-primary/20 bg-primary/[0.05] p-3"
                  >
                    <span className="min-w-0 truncate font-mono text-xs text-muted-foreground">
                      {suggestion.from}
                    </span>
                    <ArrowRight
                      aria-hidden
                      className="size-3.5 shrink-0 text-muted-foreground/40"
                    />
                    <span className="min-w-0 flex-1 truncate font-mono text-xs text-foreground">
                      {suggestion.to}
                    </span>
                    <span className="shrink-0 font-mono text-[11px] text-muted-foreground">
                      corrected {suggestion.count}×
                    </span>
                    <div className="flex shrink-0 items-center gap-1.5">
                      <Button size="xs" onClick={() => void accept(suggestion)}>
                        Add
                      </Button>
                      {/* Delete, not "never": it forgets the suggestion rather
                          than blocking the pair for good. If Piplo keeps hearing
                          it wrong the same way, it earns its way back. */}
                      <Button
                        variant="ghost"
                        size="xs"
                        title="Forget this suggestion"
                        onClick={() => void forget(suggestion)}
                      >
                        Delete
                      </Button>
                    </div>
                  </li>
                ))}
              </ul>
            </section>
          )}

          {showToolbar && (
            <div className="flex items-center gap-2 py-4">
              <div className="relative flex-1">
                <Search className="pointer-events-none absolute left-2.5 top-1/2 size-3.5 -translate-y-1/2 text-muted-foreground/50" />
                <input
                  value={query}
                  spellCheck={false}
                  placeholder="Search words and variants"
                  aria-label="Search vocabulary"
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

              {/* A segmented control rather than a <select>, for the reason on the
                  snippets page: WebView2 draws the native listbox light however
                  the page is themed. */}
              <div
                role="group"
                aria-label="Sort vocabulary"
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

              <div
                role="group"
                aria-label="Filter vocabulary"
                className="flex shrink-0 items-center gap-0.5 rounded-lg border border-input bg-white/[0.04] p-0.5"
              >
                {FILTERS.map(({ id, label }) => (
                  <button
                    key={id}
                    type="button"
                    onClick={() => setFilter(id)}
                    aria-pressed={filter === id}
                    className={cn(
                      "rounded-md px-2 py-1 font-sans text-xs transition-colors outline-none focus-visible:text-foreground",
                      filter === id
                        ? "bg-white/[0.10] text-foreground"
                        : "text-muted-foreground hover:text-foreground",
                    )}
                  >
                    {label}
                  </button>
                ))}
              </div>

              {(searching || filter !== "all") && (
                <span className="shrink-0 font-mono text-[11px] text-muted-foreground">
                  {matched.length} of {entries.length}
                </span>
              )}
            </div>
          )}

          {/* The toolbar already provides the gap under the header when it is
              there. */}
          <ul className={cn("flex flex-col gap-2", !showToolbar && "pt-4")}>
            {creating && (
              <TermForm
                entry={BLANK}
                onSave={save}
                onCancel={() => setCreating(false)}
              />
            )}

            {visible.map((entry) =>
              entry.id === editing ? (
                <TermForm
                  key={entry.id}
                  entry={entry}
                  onSave={save}
                  onCancel={() => setEditing(null)}
                />
              ) : (
                <TermRow
                  key={entry.id}
                  entry={entry}
                  onEdit={() => {
                    setCreating(false);
                    setEditing(entry.id);
                  }}
                  onDelete={() => void remove(entry.id)}
                />
              ),
            )}
          </ul>

          {loaded &&
            !creating &&
            matched.length === 0 &&
            // A filter that hides everything is not an empty dictionary, and
            // explaining what vocabulary is to someone who has 14 entries would
            // read as a bug.
            (searching || filter !== "all" ? (
              <EmptyState
                icon={SearchX}
                title={
                  searching
                    ? `Nothing matches “${query.trim()}”`
                    : `No ${filter} entries`
                }
                hint={
                  searching
                    ? "Search looks at the word and every variant, so you can find an entry by the mistake."
                    : "Piplo learns entries from the corrections you make in your history."
                }
              >
                <Button
                  variant="outline"
                  size="sm"
                  onClick={() => {
                    setQuery("");
                    setFilter("all");
                  }}
                >
                  {searching ? "Clear search" : "Show all"}
                </Button>
              </EmptyState>
            ) : (
              <NothingYet onCreate={startCreating} />
            ))}
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

/** A user with an empty list does not know what this page is for, so this
 *  explains the idea rather than just reporting the count. */
function NothingYet({ onCreate }: { onCreate: () => void }) {
  return (
    <EmptyState
      icon={BookMarked}
      title="No words yet"
      hint="Add a word Piplo keeps getting wrong — just the word. It is hinted to Whisper before the audio, and the mishearings are learned from the corrections you make in your history."
    >
      <ul className="mt-1 flex flex-col gap-1.5">
        {EXAMPLES.map(([variant, term]) => (
          <li
            key={term}
            className="flex items-center gap-2 font-mono text-[11px] text-muted-foreground/60"
          >
            <span>“{variant}”</span>
            <span aria-hidden>→</span>
            <span className="truncate">{term}</span>
          </li>
        ))}
      </ul>

      <Button variant="outline" size="sm" className="mt-2" onClick={onCreate}>
        <Plus className="size-4" />
        Add word
      </Button>
    </EmptyState>
  );
}
