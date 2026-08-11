import { useState } from "react";
import {
  Clock,
  PanelLeftClose,
  PanelLeftOpen,
  Settings as SettingsIcon,
} from "lucide-react";
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
  /* Local to the sidebar: nothing else lays out against it, and it is a view
     preference rather than one of the three saved settings. The home window is
     hidden rather than destroyed on close, so it survives until quit. */
  const [collapsed, setCollapsed] = useState(false);

  return (
    <nav
      className={cn(
        "flex shrink-0 flex-col border-r border-border bg-[#101010] p-3",
        "transition-[width] duration-200 ease-out motion-reduce:transition-none",
        collapsed ? "w-[60px]" : "w-[200px]",
      )}
    >
      <div
        className={cn(
          "mb-6 flex items-center gap-2.5 pt-2",
          collapsed ? "justify-center" : "px-2",
        )}
      >
        {/* From public/, so the same file the tray and taskbar use. No tinted
            plate behind it — the mark carries its own outline. */}
        {/* <img src="/icon.png" alt="" className="size-7 shrink-0" /> */}
        {!collapsed && (
          <span className="font-display text-sm font-semibold tracking-tight">
            Piplo
          </span>
        )}
      </div>

      <div className="flex flex-col gap-0.5">
        {NAV.map(({ id, label, icon: Icon }) => (
          <button
            key={id}
            type="button"
            onClick={() => onNavigate(id)}
            aria-label={label}
            aria-current={page === id ? "page" : undefined}
            /* The only cue left once the label is gone. */
            title={collapsed ? label : undefined}
            className={cn(
              "flex items-center gap-2.5 rounded-lg py-2 text-left font-sans text-sm transition-colors",
              collapsed ? "justify-center px-0" : "px-2.5",
              page === id
                ? "bg-white/[0.07] text-foreground"
                : "text-muted-foreground hover:bg-white/[0.04] hover:text-foreground",
            )}
          >
            <Icon className="size-4 shrink-0" />
            {!collapsed && label}
          </button>
        ))}
      </div>

      <div
        className={cn(
          "mt-auto flex items-center",
          collapsed ? "flex-col gap-2" : "justify-between",
        )}
      >
        {!collapsed && (
          <span className="px-2.5 font-mono text-[11px] text-muted-foreground/50">
            v0.1.0
          </span>
        )}
        <button
          type="button"
          onClick={() => setCollapsed((value) => !value)}
          aria-label={collapsed ? "Expand sidebar" : "Collapse sidebar"}
          aria-expanded={!collapsed}
          title={collapsed ? "Expand sidebar" : "Collapse sidebar"}
          className="flex items-center justify-center rounded-lg p-2 text-muted-foreground transition-colors hover:bg-white/[0.04] hover:text-foreground"
        >
          {collapsed ? (
            <PanelLeftOpen className="size-4" />
          ) : (
            <PanelLeftClose className="size-4" />
          )}
        </button>
      </div>
    </nav>
  );
}
