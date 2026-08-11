import MicIcon from "@/components/MicIcon";

/**
 * The chip. 1.6 gives it the morph into the recording pill; for now it is the
 * static idle shape, which is what 1.1 needs to be able to see.
 */
export default function FloatingWidget() {
  return (
    <div className="flex h-full w-full items-center justify-center">
      <div className="flex size-24 items-center justify-center rounded-full border border-border bg-background backdrop-blur-xl">
        <MicIcon className="size-7 text-muted-foreground" />
      </div>
    </div>
  );
}
