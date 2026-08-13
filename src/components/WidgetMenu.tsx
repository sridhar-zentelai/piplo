import { useCallback, useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { Home, Mic, Power, SpellCheck, Undo2 } from "lucide-react";
import { Switch } from "@/components/ui/switch";
import {
  getSettings,
  hideWidgetMenu,
  lastWordFix,
  openHome,
  quitApp,
  setSettings,
  startDictation,
  undoWordFix,
  type Settings,
  type WordFix,
} from "@/lib/commands";
import { IS_MAC } from "@/lib/shortcuts";

/** Four items, plus the word-fix undo when there is one to offer. Anything else
 *  belongs on the settings page. */
export default function WidgetMenu() {
  const [settings, setLocal] = useState<Settings | null>(null);
  const [fix, setFix] = useState<WordFix | null>(null);

  // Re-read every time the menu is summoned rather than caching: the settings
  // page is a view of the same file, and a stale switch is worse than a flicker.
  const load = useCallback(() => {
    void getSettings()
      .then(setLocal)
      .catch((error) => console.error("[piplo] could not read settings", error));

    // Same reason, and more so: what there is to undo changes with every
    // dictation.
    void lastWordFix()
      .then(setFix)
      .catch((error) => console.error("[piplo] could not read the last fix", error));
  }, []);

  useEffect(() => {
    load();

    const unlisten = listen("menu-open", load);
    return () => void unlisten.then((fn) => fn());
  }, [load]);

  async function toggleGrammar() {
    if (!settings) return;

    const next = { ...settings, grammarEnabled: !settings.grammarEnabled };
    setLocal(next);

    try {
      setLocal(await setSettings(next));
    } catch (error) {
      setLocal(settings);
      console.error("[piplo] could not save settings", error);
    }
  }

  return (
    <div className="flex h-full w-full flex-col justify-center gap-0.5 rounded-xl border border-border bg-background p-1.5 backdrop-blur-xl">
      <Item
        icon={Mic}
        label="Dictate"
        onSelect={() => {
          void startDictation();
          void hideWidgetMenu();
        }}
      />

      {/* Toggles in place. The menu deliberately stays open. */}
      <button
        type="button"
        onClick={() => void toggleGrammar()}
        className="flex items-center gap-2.5 rounded-lg px-2.5 py-2 text-left font-sans text-[13px] text-muted-foreground transition-colors hover:bg-white/[0.06] hover:text-foreground"
      >
        <SpellCheck className="size-4 shrink-0" />
        <span className="flex-1 whitespace-nowrap">Grammar correction</span>
        <Switch
          checked={settings?.grammarEnabled ?? false}
          // The row already handles the click; the switch is an indicator.
          tabIndex={-1}
          className="pointer-events-none scale-[0.8]"
        />
      </button>

      {/* Only when there is something to undo. An always-present item that
          usually does nothing teaches the user to ignore it. */}
      {fix && (
        <button
          type="button"
          onClick={() => {
            void undoWordFix().catch((error) =>
              console.error("[piplo] could not undo the word fix", error),
            );
            void hideWidgetMenu();
          }}
          className="flex items-center gap-2.5 rounded-lg px-2.5 py-2 text-left font-sans text-[13px] text-muted-foreground transition-colors hover:bg-white/[0.06] hover:text-foreground"
        >
          <Undo2 className="size-4 shrink-0" />
          <span className="min-w-0 flex-1 truncate whitespace-nowrap">
            {fix.undone ? "Redo" : "Undo"}{" "}
            <span className="font-mono text-xs">{fix.from}</span>
            <span aria-hidden> → </span>
            <span className="font-mono text-xs">{fix.to}</span>
          </span>
          {/* The same action without leaving the document, which is where it
              actually wants to be used. */}
          <span className="shrink-0 font-mono text-[10px] text-muted-foreground/50">
            {IS_MAC ? "⌘⌥Z" : "Ctrl+Alt+Z"}
          </span>
        </button>
      )}

      <div className="my-1 h-px bg-border" />

      <Item
        icon={Home}
        label="Open Piplo"
        onSelect={() => {
          void openHome();
          void hideWidgetMenu();
        }}
      />

      <Item icon={Power} label="Quit" onSelect={() => void quitApp()} />
    </div>
  );
}

function Item({
  icon: Icon,
  label,
  onSelect,
}: {
  icon: typeof Mic;
  label: string;
  onSelect: () => void;
}) {
  return (
    <button
      type="button"
      onClick={onSelect}
      className="flex items-center gap-2.5 rounded-lg px-2.5 py-2 text-left font-sans text-[13px] text-muted-foreground transition-colors hover:bg-white/[0.06] hover:text-foreground"
    >
      <Icon className="size-4 shrink-0" />
      {label}
    </button>
  );
}
