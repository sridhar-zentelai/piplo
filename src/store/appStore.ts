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
  setStatus: (status: Status) => void;
  setLevel: (level: number) => void;
};

export const useAppStore = create<AppState>((set) => ({
  status: { kind: "idle" },
  level: 0,
  setStatus: (status) => set({ status }),
  setLevel: (level) => set({ level }),
}));
