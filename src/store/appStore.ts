import { create } from "zustand";

/** Mirrors the `Status` enum in session.rs. */
export type Status =
  | { kind: "idle" }
  | { kind: "recording" }
  | { kind: "transcribing" }
  | { kind: "error"; message: string };

type AppState = {
  status: Status;
  /** RMS, 0–1. Arrives ~30×/second and is read only by Waveform. */
  level: number;
  /** The word Piplo just added on its own, while the note is on screen. `null`
   *  the rest of the time, which is nearly always. */
  learned: string | null;
  setStatus: (status: Status) => void;
  setLevel: (level: number) => void;
  setLearned: (learned: string | null) => void;
};

export const useAppStore = create<AppState>((set) => ({
  status: { kind: "idle" },
  level: 0,
  learned: null,
  setStatus: (status) => set({ status }),
  setLevel: (level) => set({ level }),
  setLearned: (learned) => set({ learned }),
}));
