import { useEffect } from "react";
import { listen } from "@tauri-apps/api/event";
import { useAppStore, type Status } from "@/store/appStore";

/**
 * The frontend hears exactly two events. `level` is kept separate from `status`
 * so a 30 Hz float never re-renders anything but the waveform.
 */
export function useWidgetEvents() {
  useEffect(() => {
    const { setStatus, setLevel } = useAppStore.getState();

    const unlisten = Promise.all([
      listen<Status>("status", (event) => setStatus(event.payload)),
      listen<number>("level", (event) => setLevel(event.payload)),
    ]);

    return () => {
      unlisten.then((fns) => fns.forEach((fn) => fn()));
    };
  }, []);
}
