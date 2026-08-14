import { useEffect } from "react";
import { listen } from "@tauri-apps/api/event";
import { useAppStore, type Status } from "@/store/appStore";

/** How long "Added X to dictionary" stays up. Longer than an error linger: it
 *  carries an Undo, and a button nobody has time to reach is not an offer. */
const NOTE_LINGER = 8000;

/**
 * `level` is kept separate from `status` so a 30 Hz float never re-renders
 * anything but the waveform. `learned` is separate from both because it arrives
 * *during* a recording — the pill goes on saying Recording underneath it, which
 * it could not do if this were a fifth status.
 */
export function useWidgetEvents() {
  useEffect(() => {
    const { setStatus, setLevel, setLearned } = useAppStore.getState();
    let dismiss: ReturnType<typeof setTimeout> | undefined;

    const unlisten = Promise.all([
      listen<Status>("status", (event) => setStatus(event.payload)),
      listen<number>("level", (event) => setLevel(event.payload)),
      listen<string>("learned", (event) => {
        // A second word inside the window replaces the first and restarts the
        // clock, rather than queueing — the note is about what just happened.
        clearTimeout(dismiss);
        setLearned(event.payload);
        dismiss = setTimeout(() => setLearned(null), NOTE_LINGER);
      }),
    ]);

    return () => {
      clearTimeout(dismiss);
      unlisten.then((fns) => fns.forEach((fn) => fn()));
    };
  }, []);
}
