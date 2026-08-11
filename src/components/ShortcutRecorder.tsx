import { useEffect, useRef, useState } from "react";
import { Button } from "@/components/ui/button";
import {
  blockedReason,
  chordFromEvent,
  formatAccelerator,
  keyLabel,
} from "@/lib/shortcuts";
import { cn } from "@/lib/utils";

/**
 * Captures a real chord rather than accepting free text, so an unparseable
 * accelerator cannot be typed in the first place.
 */
export default function ShortcutRecorder({
  value,
  onRecord,
  error,
}: {
  value: string;
  onRecord: (accelerator: string) => void;
  error: string | null;
}) {
  const [recording, setRecording] = useState(false);
  const [problem, setProblem] = useState<string | null>(null);
  const buttonRef = useRef<HTMLButtonElement>(null);

  useEffect(() => {
    if (!recording) return;

    function onKeyDown(event: KeyboardEvent) {
      // Swallow everything while recording, or Tab and Space would act on the UI
      // instead of being captured.
      event.preventDefault();
      event.stopPropagation();

      if (event.code === "Escape") {
        // Leaves the field without binding anything.
        setRecording(false);
        setProblem(null);
        return;
      }

      const capture = chordFromEvent(event);

      if (capture.status === "incomplete") return;

      if (capture.status === "unsupported") {
        setProblem(`${keyLabel(capture.code)} can't be used as a shortcut.`);
        return;
      }

      const blocked = blockedReason(capture.chord);

      if (blocked) {
        setProblem(blocked);
        return;
      }

      setRecording(false);
      setProblem(null);
      onRecord(formatAccelerator(capture.chord));
    }

    window.addEventListener("keydown", onKeyDown, { capture: true });
    return () =>
      window.removeEventListener("keydown", onKeyDown, { capture: true });
  }, [recording, onRecord]);

  return (
    <div className="flex flex-col items-end gap-1.5">
      <Button
        ref={buttonRef}
        variant="outline"
        onClick={() => {
          setProblem(null);
          setRecording((on) => !on);
        }}
        onBlur={() => setRecording(false)}
        className={cn(
          "min-w-[140px] font-mono text-xs",
          recording && "border-primary text-primary",
        )}
      >
        {recording ? "Press a chord…" : value}
      </Button>

      {(problem ?? error) && (
        <p className="max-w-[280px] text-right font-sans text-[11px] leading-snug text-destructive">
          {problem ?? error}
        </p>
      )}
    </div>
  );
}
