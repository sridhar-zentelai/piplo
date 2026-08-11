import { useEffect, useState } from "react";
import MicIcon from "@/components/MicIcon";
import { useWidgetEvents } from "@/hooks/useWidgetEvents";
import { getSettings, type HistoryEntry } from "@/lib/commands";
import { useAppStore } from "@/store/appStore";
import { cn } from "@/lib/utils";

export default function HomeHero({ entries }: { entries: HistoryEntry[] }) {
  // The home window listens to the same two events the widget does, so this
  // status is the live one rather than a guess.
  useWidgetEvents();

  const [shortcut, setShortcut] = useState<string | null>(null);

  useEffect(() => {
    void getSettings()
      .then((settings) => setShortcut(settings.shortcut))
      .catch(() => setShortcut(null));
  }, []);

  return (
    <section className="border-b border-border px-6 pb-6 pt-7">
      <div className="flex items-start justify-between gap-6">
        <div className="flex min-w-0 items-center gap-4">
          <span className="flex size-12 shrink-0 items-center justify-center rounded-2xl bg-primary/15 text-primary">
            <MicIcon className="size-6" />
          </span>

          <div className="min-w-0">
            <h1 className="flex items-center gap-2.5 font-display text-2xl font-semibold tracking-tight">
              Hello, <span className="text-primary">Piplo</span>
              <span className="rounded-full border border-border bg-white/[0.04] px-2 py-0.5 font-mono text-[11px] font-normal text-muted-foreground">
                v0.1.0
              </span>
            </h1>

            <p className="mt-1.5 flex flex-wrap items-center gap-1.5 font-sans text-sm text-muted-foreground">
              Hold
              {shortcut ? (
                shortcut.split("+").map((key, i) => (
                  <span key={`${key}-${i}`} className="flex items-center gap-1.5">
                    {i > 0 && <span className="text-muted-foreground/50">+</span>}
                    <kbd className="rounded-md border border-border bg-white/[0.06] px-1.5 py-0.5 font-mono text-[11px] text-foreground">
                      {key}
                    </kbd>
                  </span>
                ))
              ) : (
                <span className="text-muted-foreground/50">…</span>
              )}
              to dictate anywhere.
            </p>
          </div>
        </div>

        <StatusPill />
      </div>

      <div className="mt-6 grid grid-cols-1 gap-3 sm:grid-cols-3">
        <Stat label="Dictations" value={entries.length.toLocaleString()} />
        <Stat label="Total words" value={countWords(entries).toLocaleString()} />
        <Stat label="Day streak" value={dayStreak(entries)} unit="day" />
      </div>
    </section>
  );
}

function StatusPill() {
  const status = useAppStore((state) => state.status);

  const { label, tone } = {
    idle: { label: "Ready", tone: "bg-primary" },
    recording: { label: "Recording", tone: "bg-primary animate-pulse" },
    transcribing: { label: "Working", tone: "bg-primary animate-pulse" },
    error: { label: "Error", tone: "bg-destructive" },
  }[status.kind];

  return (
    <div className="flex shrink-0 items-center gap-2">
      <span className={cn("size-2 rounded-full", tone)} />
      <span className="font-sans text-sm text-foreground">
        {status.kind === "error" ? status.message : label}
      </span>
    </div>
  );
}

function Stat({
  label,
  value,
  unit,
}: {
  label: string;
  value: string | number;
  unit?: string;
}) {
  return (
    <div className="rounded-xl border border-border bg-white/[0.02] px-4 py-3.5">
      <p className="font-sans text-[11px] uppercase tracking-widest text-muted-foreground">
        {label}
      </p>
      <p className="mt-1 font-display text-2xl font-semibold tracking-tight">
        {value}
        {unit && (
          <span className="ml-1.5 font-sans text-sm font-normal text-muted-foreground">
            {typeof value === "number" && value === 1 ? unit : `${unit}s`}
          </span>
        )}
      </p>
    </div>
  );
}

function countWords(entries: HistoryEntry[]): number {
  return entries.reduce((total, entry) => {
    const words = entry.text.trim();
    return total + (words === "" ? 0 : words.split(/\s+/).length);
  }, 0);
}

/**
 * Consecutive local days with at least one dictation, counting back from today.
 * A gap yesterday ends the streak; a gap today does not, so the number does not
 * reset the moment midnight passes.
 */
function dayStreak(entries: HistoryEntry[]): number {
  const days = new Set<string>();

  for (const entry of entries) {
    const at = new Date(entry.at);
    if (!Number.isNaN(at.getTime())) days.add(localDay(at));
  }

  if (days.size === 0) return 0;

  const cursor = new Date();

  if (!days.has(localDay(cursor))) {
    cursor.setDate(cursor.getDate() - 1);
    if (!days.has(localDay(cursor))) return 0;
  }

  let streak = 0;

  while (days.has(localDay(cursor))) {
    streak += 1;
    cursor.setDate(cursor.getDate() - 1);
  }

  return streak;
}

/** Local calendar day, not UTC — a 1 a.m. dictation belongs to the day you felt it was. */
function localDay(date: Date): string {
  return `${date.getFullYear()}-${date.getMonth()}-${date.getDate()}`;
}
