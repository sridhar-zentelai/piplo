import { useEffect } from "react";
import { motion } from "framer-motion";
import PillContents from "@/components/PillContents";
import { useWidgetEvents } from "@/hooks/useWidgetEvents";
import { showWidgetMenu, widgetSetActive } from "@/lib/commands";
import { useAppStore } from "@/store/appStore";
import { cn } from "@/lib/utils";

/** `transcribing` and `error` reuse the recording geometry on purpose — a third
 *  set of dimensions would introduce a mid-flight morph that reads as a glitch. */
const SHAPE = {
  idle: { width: 56, height: 56, borderRadius: 28 },
  recording: { width: 260, height: 56, borderRadius: 28 },
  transcribing: { width: 260, height: 56, borderRadius: 28 },
  error: { width: 260, height: 56, borderRadius: 28 },
} as const;

const MORPH = { type: "spring", stiffness: 400, damping: 32 } as const;

export default function FloatingWidget() {
  useWidgetEvents();
  const status = useAppStore((state) => state.status);
  const idle = status.kind === "idle";

  // Widen the window before the pill grows, so it has room to animate into.
  // Narrowing waits for onAnimationComplete instead.
  useEffect(() => {
    if (!idle) void widgetSetActive(true);
  }, [idle]);

  return (
    <div
      className="flex h-full w-full items-center justify-center"
      onContextMenu={() => void showWidgetMenu()}
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
