import { useEffect, useRef, useState } from "react";
import { useAppStore } from "@/store/appStore";

const BARS = 16;
/** Without the decay the bars flicker between frames and read as noise. */
const DECAY = 0.86;
/** So the pill never looks broken in silence. */
const MIN_HEIGHT = 0.12;

export default function Waveform() {
  const level = useAppStore((state) => state.level);
  const [bars, setBars] = useState<number[]>(() => Array(BARS).fill(MIN_HEIGHT));
  const latest = useRef(level);

  latest.current = level;

  useEffect(() => {
    const id = setInterval(() => {
      setBars((previous) => {
        const next = Math.max(latest.current, previous[previous.length - 1] * DECAY);
        return [...previous.slice(1), Math.max(next, MIN_HEIGHT)];
      });
    }, 33);

    return () => clearInterval(id);
  }, []);

  return (
    <div className="flex h-4 flex-1 items-center justify-center gap-[2px]">
      {bars.map((height, i) => (
        <span
          key={i}
          className="w-[2px] shrink-0 rounded-full bg-primary"
          style={{ height: `${Math.round(height * 100)}%` }}
        />
      ))}
    </div>
  );
}
