import { useState } from "react";
import Sidebar, { type Page } from "@/components/Sidebar";
import HomePage from "@/pages/HomePage";
import SettingsPage from "@/pages/SettingsPage";
import SnippetsPage from "@/pages/SnippetsPage";
import VocabularyPage from "@/pages/VocabularyPage";

/**
 * No router. Four views with no URLs and no deep linking would make a router an
 * abstraction with a single call site.
 */
export default function DesktopWindow() {
  const [page, setPage] = useState<Page>("home");

  return (
    <div className="flex h-full bg-[#141414] text-foreground">
      <Sidebar page={page} onNavigate={setPage} />
      <main className="flex min-w-0 flex-1 flex-col">
        {page === "home" ? (
          <HomePage />
        ) : page === "vocabulary" ? (
          <VocabularyPage />
        ) : page === "snippets" ? (
          <SnippetsPage />
        ) : (
          <SettingsPage />
        )}
      </main>
    </div>
  );
}
