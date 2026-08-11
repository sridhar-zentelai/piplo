import { ChevronLeft, ChevronRight } from "lucide-react";
import { Button } from "@/components/ui/button";

export default function Pagination({
  page,
  pageCount,
  from,
  to,
  total,
  onChange,
}: {
  /** 1-based. */
  page: number;
  pageCount: number;
  from: number;
  to: number;
  total: number;
  onChange: (page: number) => void;
}) {
  return (
    <div className="flex items-center justify-between gap-4 border-t border-border px-6 py-3">
      <span className="font-mono text-[11px] text-muted-foreground">
        {from}–{to} of {total}
      </span>

      <div className="flex items-center gap-1.5">
        <Button
          variant="ghost"
          size="icon"
          aria-label="Previous page"
          disabled={page <= 1}
          onClick={() => onChange(page - 1)}
        >
          <ChevronLeft className="size-4" />
        </Button>

        <span className="min-w-[70px] text-center font-mono text-[11px] text-muted-foreground">
          {page} / {pageCount}
        </span>

        <Button
          variant="ghost"
          size="icon"
          aria-label="Next page"
          disabled={page >= pageCount}
          onClick={() => onChange(page + 1)}
        >
          <ChevronRight className="size-4" />
        </Button>
      </div>
    </div>
  );
}
