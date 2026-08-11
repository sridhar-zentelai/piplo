import SettingsPanel from "@/components/SettingsPanel";

export default function SettingsPage() {
  return (
    <>
      <header className="border-b border-border px-6 py-4">
        <h1 className="font-display text-lg font-semibold tracking-tight">
          Settings
        </h1>
      </header>

      <div className="min-h-0 flex-1 overflow-y-auto px-6">
        <SettingsPanel />
      </div>
    </>
  );
}
