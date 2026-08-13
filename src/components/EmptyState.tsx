/**
 * The centred nothing-here block, shared by the history and snippets lists.
 * Extracted rather than built twice — if it needs to change for one list it
 * changes for both.
 *
 * `children` is for an action (a button, a link) below the hint. Everything above
 * it is text, so a caller cannot accidentally invent a second layout.
 */
export default function EmptyState({
  icon: Icon,
  title,
  hint,
  children,
}: {
  /** A lucide icon or Piplo's own mark — both take just a className. */
  icon: React.ComponentType<{ className?: string }>;
  title: string;
  hint?: string;
  children?: React.ReactNode;
}) {
  return (
    <div className="flex flex-col items-center justify-center gap-3 py-16 text-center">
      <Icon className="size-8 text-muted-foreground/40" />
      <p className="font-sans text-sm text-muted-foreground">{title}</p>
      {hint && (
        <p className="max-w-[380px] font-sans text-xs leading-relaxed text-muted-foreground/70">
          {hint}
        </p>
      )}
      {children}
    </div>
  );
}
