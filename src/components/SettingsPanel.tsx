import { useCallback, useEffect, useState } from "react";
import { AlertTriangle } from "lucide-react";
import ShortcutRecorder from "@/components/ShortcutRecorder";
import { Switch } from "@/components/ui/switch";
import { getSettings, setSettings, type Settings } from "@/lib/commands";
import { warningFor } from "@/lib/shortcuts";

export default function SettingsPanel() {
  const [settings, setLocal] = useState<Settings | null>(null);
  const [shortcutError, setShortcutError] = useState<string | null>(null);

  useEffect(() => {
    void getSettings()
      .then(setLocal)
      .catch((error) => console.error("[piplo] could not read settings", error));
  }, []);

  /**
   * Optimistic, then rolled back on rejection. A refused shortcut leaves both the
   * live binding and the saved file untouched, so the UI has to follow.
   */
  const apply = useCallback(
    async (next: Settings) => {
      const previous = settings;
      setLocal(next);
      setShortcutError(null);

      try {
        setLocal(await setSettings(next));
      } catch (error) {
        setLocal(previous);
        setShortcutError(String(error));
      }
    },
    [settings],
  );

  if (!settings) return null;

  const warning = warningFor(settings.shortcut);

  return (
    <div className="flex flex-col divide-y divide-border">
      <Row
        label="Shortcut"
        hint="Hold it to dictate. Release to type."
        warning={warning}
      >
        <ShortcutRecorder
          value={settings.shortcut}
          error={shortcutError}
          onRecord={(shortcut) => void apply({ ...settings, shortcut })}
        />
      </Row>

      <Row
        label="Automatic grammar correction"
        hint="Cleans up punctuation and fillers before typing. Applies to the next dictation."
      >
        <Switch
          checked={settings.grammarEnabled}
          onCheckedChange={(grammarEnabled) =>
            void apply({ ...settings, grammarEnabled })
          }
        />
      </Row>

      <Row
        label="Show floating widget"
        hint="Off hides the chip. The shortcut keeps working and the pill still appears while you dictate."
      >
        <Switch
          checked={settings.widgetVisible}
          onCheckedChange={(widgetVisible) =>
            void apply({ ...settings, widgetVisible })
          }
        />
      </Row>
    </div>
  );
}

function Row({
  label,
  hint,
  warning,
  children,
}: {
  label: string;
  hint: string;
  warning?: string | null;
  children: React.ReactNode;
}) {
  return (
    <div className="flex items-start justify-between gap-8 py-5">
      <div className="min-w-0">
        <p className="font-sans text-sm text-foreground">{label}</p>
        <p className="mt-1 max-w-[420px] font-sans text-xs leading-relaxed text-muted-foreground">
          {hint}
        </p>
        {warning && (
          <p className="mt-2 flex max-w-[420px] items-start gap-1.5 font-sans text-xs leading-relaxed text-muted-foreground">
            <AlertTriangle className="mt-px size-3.5 shrink-0 text-[#E0B341]" />
            {warning}
          </p>
        )}
      </div>
      <div className="shrink-0 pt-0.5">{children}</div>
    </div>
  );
}
