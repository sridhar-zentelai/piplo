import { useCallback, useEffect, useState } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { getHistory, type HistoryEntry } from "@/lib/commands";

/**
 * Fetches on mount and again whenever the window is shown. Dictations happen
 * while this window is hidden, so a stale list is the normal case rather than an
 * edge case — and pushing from Rust on every dictation would be work nobody sees.
 */
export function useHistory() {
  const [entries, setEntries] = useState<HistoryEntry[]>([]);
  const [loading, setLoading] = useState(true);

  const refresh = useCallback(async () => {
    try {
      setEntries(await getHistory());
    } catch (error) {
      console.error("[piplo] could not read history", error);
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    void refresh();

    const unlisten = getCurrentWindow().onFocusChanged(({ payload }) => {
      if (payload) void refresh();
    });

    return () => {
      void unlisten.then((fn) => fn());
    };
  }, [refresh]);

  return { entries, loading, refresh };
}
