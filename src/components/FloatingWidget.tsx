import { useEffect } from "react";
import { motion } from "framer-motion";
import PillContents from "@/components/PillContents";
import { useWidgetDrag } from "@/hooks/useWidgetDrag";
import { useWidgetEvents } from "@/hooks/useWidgetEvents";
import { showWidgetMenu, widgetSetActive } from "@/lib/commands";
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
  const idle = status.kind === "idle";

  // Widen the window before the pill grows, so it has room to animate into.
  // Narrowing waits for onAnimationComplete instead.
  useEffect(() => {
    if (!idle) void widgetSetActive(true);
  }, [idle]);

  return (
    <div
      className="flex h-full w-full select-none items-center justify-center"
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
          if (idle) void widgetSetActive(false);
        }}
        className={cn(
          "flex items-center justify-center overflow-hidden border bg-background backdrop-blur-xl",
          status.kind === "error" ? "border-destructive" : "border-border",
          (status.kind === "recording" || status.kind === "transcribing") &&
            "border-primary",
        )}
      >
        <PillContents status={status} />
      </motion.div>
    </div>
  );
}
