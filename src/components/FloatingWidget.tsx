import { useEffect } from "react";
import { AnimatePresence, motion } from "framer-motion";
import LearnedNote from "@/components/LearnedNote";
import PillContents from "@/components/PillContents";
import { useWidgetDrag } from "@/hooks/useWidgetDrag";
import { useWidgetEvents } from "@/hooks/useWidgetEvents";
import { showWidgetMenu, widgetSetShape } from "@/lib/commands";
import { useAppStore } from "@/store/appStore";
import { cn } from "@/lib/utils";

/** `transcribing` and `error` reuse the recording geometry on purpose — a third
 *  set of dimensions would introduce a mid-flight morph that reads as a glitch. */
const SHAPE = {
  idle: { width: 40, height: 40, borderRadius: 20 },
  recording: { width: 200, height: 40, borderRadius: 20 },
  transcribing: { width: 200, height: 40, borderRadius: 20 },
  error: { width: 200, height: 40, borderRadius: 20 },
} as const;

const MORPH = { type: "spring", stiffness: 400, damping: 32 } as const;

export default function FloatingWidget() {
  useWidgetEvents();
  const drag = useWidgetDrag();
  const status = useAppStore((state) => state.status);
  const learned = useAppStore((state) => state.learned);
  const idle = status.kind === "idle";

  // Grow the window before the contents grow, so they have room to animate into.
  // Shrinking waits for onAnimationComplete instead. The note wins over the
  // status: it is the taller of the two and clipping it would lose the Undo.
  useEffect(() => {
    if (learned) void widgetSetShape("note");
    else if (!idle) void widgetSetShape("pill");
  }, [idle, learned]);

  return (
    <div
      className="flex h-full w-full select-none flex-col items-center justify-center gap-2"
      onContextMenu={() => void showWidgetMenu()}
      {...drag.handlers}
      // The press that ended a drag must not also press whatever was under it.
      onClickCapture={(event) => {
        if (!drag.dragged()) return;
        event.preventDefault();
        event.stopPropagation();
      }}
    >
      <motion.div
        animate={SHAPE[status.kind]}
        transition={MORPH}
        onAnimationComplete={() => {
          // Only when there is nothing else in the window to clip.
          if (idle && !learned) void widgetSetShape("idle");
        }}
        className={cn(
          "flex shrink-0 items-center justify-center overflow-hidden border bg-background backdrop-blur-xl",
          status.kind === "error" ? "border-destructive" : "border-border",
          (status.kind === "recording" || status.kind === "transcribing") &&
            "border-primary",
        )}
      >
        <PillContents status={status} />
      </motion.div>

      <AnimatePresence
        // The window can only narrow once the note has finished leaving it.
        onExitComplete={() => {
          if (idle) void widgetSetShape("idle");
        }}
      >
        {learned && <LearnedNote term={learned} />}
      </AnimatePresence>
    </div>
  );
}
