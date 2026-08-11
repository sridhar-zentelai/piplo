import { useEffect, useState } from "react";
import MicIcon from "@/components/MicIcon";
import Waveform from "@/components/Waveform";
import type { Status } from "@/store/appStore";

export default function PillContents({ status }: { status: Status }) {
  if (status.kind === "idle") {
    return <MicIcon className="size-5 text-muted-foreground" />;
  }

  return (
    <div className="flex w-full items-center gap-3 px-4">
      {/* ✕ stays live as an abort through the whole flow. Wired up in 2.6. */}
      <Glyph label="Cancel">
        <path d="M5 5l10 10M15 5L5 15" />
      </Glyph>

      {status.kind === "recording" && (
        <>
          <Waveform />
          <Elapsed />
          {/* ✓ is hidden while transcribing — nothing left to accept. */}
          <Glyph label="Accept">
            <path d="M4 11l4 4 8-9" />
          </Glyph>
        </>
      )}

      {status.kind === "transcribing" && <Dots />}

      {status.kind === "error" && (
        <p className="flex-1 truncate font-sans text-sm text-destructive">
          {status.message}
        </p>
      )}
    </div>
  );
}

function Glyph({
  label,
  children,
}: {
  label: string;
  children: React.ReactNode;
}) {
  return (
    <svg
      viewBox="0 0 20 20"
      fill="none"
      stroke="currentColor"
      strokeWidth={1.75}
      strokeLinecap="round"
      className="size-4 shrink-0 text-muted-foreground"
      role="img"
      aria-label={label}
    >
      {children}
    </svg>
  );
}

function Elapsed() {
  const [seconds, setSeconds] = useState(0);

  useEffect(() => {
    const id = setInterval(() => setSeconds((s) => s + 1), 1000);
    return () => clearInterval(id);
  }, []);

  const mm = Math.floor(seconds / 60);
  const ss = String(seconds % 60).padStart(2, "0");

  return (
    <span className="shrink-0 font-mono text-xs tabular-nums text-muted-foreground">
      {mm}:{ss}
    </span>
  );
}

function Dots() {
  return (
    <div className="flex flex-1 items-center justify-center gap-1.5">
      {[0, 1, 2].map((i) => (
        <span
          key={i}
          className="size-1.5 animate-pulse rounded-full bg-primary"
          style={{ animationDelay: `${i * 160}ms` }}
        />
      ))}
    </div>
  );
}
