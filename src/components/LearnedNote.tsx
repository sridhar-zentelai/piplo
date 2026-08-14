import { useState } from "react";
import { motion } from "framer-motion";
import { Sparkles } from "lucide-react";
import { undoLearnedTerm } from "@/lib/commands";
import { useAppStore } from "@/store/appStore";

/**
 * "Added ZentelAI to dictionary", under the pill, when Piplo noticed the user had
 * fixed the last dictation by hand.
 *
 * It appears at the *start* of a recording, so it sits beside the pill rather
 * than replacing it — the user is mid-sentence and the recording state is the one
 * thing that must stay legible. Clicks land here even though the widget never
 * takes focus: it is `WS_EX_NOACTIVATE`, not click-through, which is what already
 * makes the mic button work.
 */
export default function LearnedNote({ term }: { term: string }) {
  const setLearned = useAppStore((state) => state.setLearned);
  const [busy, setBusy] = useState(false);

  async function undo() {
    if (busy) return;
    setBusy(true);

    try {
      await undoLearnedTerm(term);
    } catch (error) {
      // The note goes either way. It is a transient offer, not a form: leaving it
      // up with a failed Undo would be a second thing the user cannot act on.
      console.error("[piplo] could not undo the learned word", error);
    }

    setLearned(null);
  }

  return (
    <motion.div
      initial={{ opacity: 0, y: -6, scale: 0.95 }}
      animate={{ opacity: 1, y: 0, scale: 1 }}
      exit={{ opacity: 0, scale: 0.95, transition: { duration: 0.15 } }}
      transition={{ duration: 0.2 }}
      className="flex max-w-full shrink-0 items-center gap-2 rounded-xl border border-primary/30 bg-background px-3 py-2 backdrop-blur-xl"
    >
      <Sparkles aria-hidden className="size-3.5 shrink-0 text-primary" />

      <p className="min-w-0 truncate font-sans text-xs text-foreground">
        Added <span className="font-mono">{term}</span> to dictionary
      </p>

      <button
        type="button"
        onClick={() => void undo()}
        disabled={busy}
        className="ml-auto shrink-0 rounded-md px-1.5 py-0.5 font-sans text-xs text-muted-foreground transition-colors hover:bg-white/[0.06] hover:text-foreground disabled:opacity-50"
      >
        Undo
      </button>
    </motion.div>
  );
}
