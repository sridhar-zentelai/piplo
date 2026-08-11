import { useEffect, useState } from "react";
import MicIcon from "@/components/MicIcon";
import Waveform from "@/components/Waveform";
import { cancelDictation, finishDictation, startDictation } from "@/lib/commands";
import type { Status } from "@/store/appStore";

export default function PillContents({ status }: { status: Status }) {
  if (status.kind === "idle") {
    return (
      <button
        type="button"
        onClick={() => void startDictation()}
        aria-label="Dictate"
        className="flex size-full items-center justify-center text-muted-foreground transition-colors hover:text-foreground"
      >
        <MicIcon className="size-5" />
      </button>
    );
  }

  return (
    <div className="flex w-full items-center gap-3 px-4">
      {/* ✕ stays live as an abort through the whole flow, both network calls
          included. */}
      <Glyph label="Cancel" onClick={() => void cancelDictation()}>
        <path d="M5 5l10 10M15 5L5 15" />
      </Glyph>

      {status.kind === "recording" && (
        <>
          <Waveform />
          <Elapsed />
          {/* ✓ is hidden while transcribing — there is nothing left to accept. */}
          <Glyph label="Accept" onClick={() => void finishDictation()}>
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
  onClick,
  children,
}: {
  label: string;
  onClick: () => void;
  children: React.ReactNode;
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      aria-label={label}
      className="shrink-0 text-muted-foreground transition-colors hover:text-foreground"
    >
      <svg
        viewBox="0 0 20 20"
        fill="none"
        stroke="currentColor"
        strokeWidth={1.75}
        strokeLinecap="round"
        className="size-4"
        aria-hidden
      >
        {children}
      </svg>
    </button>
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
