import SettingsPanel from "@/components/SettingsPanel";

export default function SettingsPage() {
  return (
    <>
      {/* The rule spans the window; the heading lines up with the content below it. */}
      <header className="border-b border-border">
        <div className="mx-auto w-full max-w-3xl px-6 py-4">
          <h1 className="font-display text-lg font-semibold tracking-tight">
            Settings
          </h1>
        </div>
      </header>

      <div className="min-h-0 flex-1 overflow-y-auto">
        <div className="mx-auto w-full max-w-3xl px-6">
          <SettingsPanel />
        </div>
      </div>
    </>
  );
}
