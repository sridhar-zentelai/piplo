import { Clock, Settings as SettingsIcon } from "lucide-react";
import MicIcon from "@/components/MicIcon";
import { cn } from "@/lib/utils";

export type Page = "home" | "settings";

const NAV: { id: Page; label: string; icon: typeof Clock }[] = [
  { id: "home", label: "Home", icon: Clock },
  { id: "settings", label: "Settings", icon: SettingsIcon },
];

export default function Sidebar({
  page,
  onNavigate,
}: {
  page: Page;
  onNavigate: (page: Page) => void;
}) {
  return (
    <nav className="flex w-[200px] shrink-0 flex-col border-r border-border bg-[#101010] p-3">
      <div className="mb-6 flex items-center gap-2.5 px-2 pt-2">
        <span className="flex size-7 items-center justify-center rounded-lg bg-primary/15 text-primary">
          <MicIcon className="size-4" />
        </span>
        <span className="font-display text-sm font-semibold tracking-tight">
          Piplo
        </span>
      </div>

      <div className="flex flex-col gap-0.5">
        {NAV.map(({ id, label, icon: Icon }) => (
          <button
            key={id}
            type="button"
            onClick={() => onNavigate(id)}
            className={cn(
              "flex items-center gap-2.5 rounded-lg px-2.5 py-2 text-left font-sans text-sm transition-colors",
              page === id
                ? "bg-white/[0.07] text-foreground"
                : "text-muted-foreground hover:bg-white/[0.04] hover:text-foreground",
            )}
          >
            <Icon className="size-4 shrink-0" />
            {label}
          </button>
        ))}
      </div>

      <span className="mt-auto px-2.5 font-mono text-[11px] text-muted-foreground/50">
        v0.1.0
      </span>
    </nav>
  );
}
