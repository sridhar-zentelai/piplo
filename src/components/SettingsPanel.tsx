import { useCallback, useEffect, useState } from "react";
import { AlertTriangle } from "lucide-react";
import ShortcutRecorder from "@/components/ShortcutRecorder";
import { Button } from "@/components/ui/button";
import { Switch } from "@/components/ui/switch";
import {
  clearApiKey,
  getApiKeyStatus,
  getSettings,
  listShortcuts,
  setApiKey,
  setSettings,
  type ApiKeyStatus,
  type Binding,
  type Settings,
} from "@/lib/commands";
import { warningFor } from "@/lib/shortcuts";
import { cn } from "@/lib/utils";

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

      {/* Only the learning half gets a switch. The words the user typed in do not
          need one — a toggle that ignores what you typed is a worse control than
          deleting the entry. */}
      <Row
        label="Learn from my corrections"
        hint="When you fix a dictation in your history, Piplo works out which word it got wrong and remembers it after the third time. Nothing outside Piplo is watched."
      >
        <Switch
          checked={settings.learnFromCorrections}
          onCheckedChange={(learnFromCorrections) =>
            void apply({ ...settings, learnFromCorrections })
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

      <ApiKeyRow />
      <ShortcutList />
    </div>
  );
}

/**
 * The fixed shortcuts, read-only.
 *
 * Not editable, and not a fifth and sixth setting: they are two chords that only
 * make sense right after a dictation, and a picker for each would be more surface
 * than the feature is worth. Listed because a shortcut nobody can find is a
 * shortcut nobody uses.
 */
function ShortcutList() {
  const [bindings, setBindings] = useState<Binding[]>([]);

  useEffect(() => {
    void listShortcuts()
      .then(setBindings)
      .catch((error) =>
        console.error("[piplo] could not read the shortcuts", error),
      );
  }, []);

  if (bindings.length === 0) return null;

  return (
    <div className="py-5">
      <p className="font-sans text-sm text-foreground">Other shortcuts</p>
      <p className="mt-1 max-w-[420px] font-sans text-xs leading-relaxed text-muted-foreground">
        Fixed, and they act on the last thing Piplo typed — press them while your
        caret is still where the text landed.
      </p>

      <dl className="mt-3 flex flex-col gap-2.5">
        {bindings.map((binding) => (
          <div key={binding.accelerator} className="flex items-start gap-3">
            {/* Fixed-width so the descriptions line up whatever the chord. */}
            <dt className="w-[104px] shrink-0">
              <kbd
                className={cn(
                  "rounded-md border border-input bg-white/[0.04] px-1.5 py-0.5 font-mono text-[11px]",
                  binding.bound
                    ? "text-foreground"
                    : "text-muted-foreground/50 line-through",
                )}
              >
                {binding.accelerator}
              </kbd>
            </dt>
            <dd className="min-w-0">
              <p className="font-sans text-xs text-foreground">
                {binding.label}
              </p>
              <p className="mt-0.5 max-w-[380px] font-sans text-xs leading-relaxed text-muted-foreground">
                {binding.hint}
              </p>
              {/* Said plainly rather than left to look broken: another app owns
                  the chord, and Piplo cannot take it. */}
              {!binding.bound && (
                <p className="mt-1 font-sans text-xs text-muted-foreground">
                  Another app has claimed this one, so it does nothing here.
                </p>
              )}
            </dd>
          </div>
        ))}
      </dl>
    </div>
  );
}

function ApiKeyRow() {
  const [status, setStatus] = useState<ApiKeyStatus | null>(null);
  /** Null unless the field is open, so a saved key leaves nothing in the DOM. */
  const [draft, setDraft] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    void getApiKeyStatus()
      .then(setStatus)
      .catch((cause) => {
        console.error("[piplo] could not read the API key status", cause);
        setError("Not available in this build.");
      });
  }, []);

  async function run(action: () => Promise<ApiKeyStatus>) {
    setBusy(true);
    setError(null);
    try {
      setStatus(await action());
      setDraft(null);
    } catch (cause) {
      // Never the key itself, only whatever Rust chose to say.
      setError(String(cause));
    } finally {
      setBusy(false);
    }
  }

  const fromEnv = status?.source === "env";

  return (
    <Row
      label="Groq API key"
      hint="Used for transcription and grammar. Stored by Piplo and never shown again once saved."
      warning={
        fromEnv
          ? "GROQ_API_KEY is set in the environment and takes priority. Remove it from .env to use a key saved here."
          : null
      }
    >
      <div className="flex w-[280px] flex-col items-end gap-2">
        {draft === null ? (
          <div className="flex items-center gap-2">
            <span className="font-mono text-xs text-muted-foreground">
              {status === null
                ? "…"
                : status.configured
                  ? `••••${status.hint ?? ""}`
                  : "Not set"}
            </span>
            <Button
              variant="outline"
              size="sm"
              disabled={busy || fromEnv || status === null}
              onClick={() => {
                setError(null);
                setDraft("");
              }}
            >
              {status?.configured ? "Change" : "Add"}
            </Button>
            {status?.configured && !fromEnv && (
              <Button
                variant="ghost"
                size="sm"
                disabled={busy}
                onClick={() => void run(clearApiKey)}
              >
                Remove
              </Button>
            )}
          </div>
        ) : (
          <>
            <input
              type="password"
              value={draft}
              autoFocus
              spellCheck={false}
              autoComplete="off"
              placeholder="gsk_…"
              aria-label="Groq API key"
              disabled={busy}
              onChange={(event) => setDraft(event.target.value)}
              onKeyDown={(event) => {
                if (event.key === "Enter" && draft.trim() !== "") {
                  void run(() => setApiKey(draft.trim()));
                }
                if (event.key === "Escape") {
                  setDraft(null);
                  setError(null);
                }
              }}
              className="w-full rounded-lg border border-input bg-white/[0.04] px-2.5 py-1.5 font-mono text-xs text-foreground outline-none placeholder:text-muted-foreground/50 focus-visible:border-ring"
            />
            <div className="flex items-center gap-2">
              <Button
                variant="ghost"
                size="sm"
                disabled={busy}
                onClick={() => {
                  setDraft(null);
                  setError(null);
                }}
              >
                Cancel
              </Button>
              <Button
                size="sm"
                // Trimmed: a pasted key usually arrives with a newline.
                disabled={busy || draft.trim() === ""}
                onClick={() => void run(() => setApiKey(draft.trim()))}
              >
                Save
              </Button>
            </div>
          </>
        )}

        {draft !== null && draft.trim() !== "" && !draft.startsWith("gsk_") && (
          <p className="text-right font-sans text-xs text-muted-foreground">
            Groq keys normally start with <code className="font-mono">gsk_</code>.
          </p>
        )}

        {error && (
          <p className="text-right font-sans text-xs text-destructive">{error}</p>
        )}
      </div>
    </Row>
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
