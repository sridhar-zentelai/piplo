import { useState } from "react";
import { Check, Copy } from "lucide-react";
import { writeText } from "@tauri-apps/plugin-clipboard-manager";
import type { HistoryEntry } from "@/lib/commands";
import { cn } from "@/lib/utils";

export default function HistoryRow({ entry }: { entry: HistoryEntry }) {
  const [expanded, setExpanded] = useState(false);
  const [copied, setCopied] = useState(false);

  async function copy() {
    try {
      // The plugin, not navigator.clipboard: the packaged app serves
      // http://tauri.localhost, which is not a secure context, so the browser API
      // is unavailable — and it works under `tauri dev`, so getting this wrong
      // breaks only in the build.
      await writeText(entry.text);
      setCopied(true);
      setTimeout(() => setCopied(false), 1400);
    } catch (error) {
      console.error("[piplo] copy failed", error);
    }
  }

  return (
    <li className="group rounded-xl border border-border bg-white/[0.02] p-3.5 transition-colors hover:border-white/[0.14]">
      <div className="flex items-start gap-3">
        <button
          type="button"
          onClick={() => setExpanded((open) => !open)}
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
            {entry.text}
          </p>
        </button>

        <button
          type="button"
          onClick={copy}
          aria-label={copied ? "Copied" : "Copy"}
          className="shrink-0 rounded-md p-1.5 text-muted-foreground opacity-0 transition-all hover:bg-white/[0.06] hover:text-foreground focus-visible:opacity-100 group-hover:opacity-100"
        >
          {copied ? (
            <Check className="size-4 text-primary" />
          ) : (
            <Copy className="size-4" />
          )}
        </button>
      </div>

      <p className="mt-2 font-mono text-[11px] text-muted-foreground/70">
        {relativeTime(entry.at)}
        {" · "}
        {entry.duration_secs.toFixed(1)}s
        {entry.language ? ` · ${entry.language}` : ""}
      </p>
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
