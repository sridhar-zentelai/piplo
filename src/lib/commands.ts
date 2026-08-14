import { invoke } from "@tauri-apps/api/core";

/** Mirrors `history::Entry`. */
export interface HistoryEntry {
  id: string;
  /** RFC 3339, UTC. */
  at: string;
  duration_secs: number;
  model: string;
  language: string | null;
  chars: number;
  inserted: boolean;
  text: string;
  /** Present only when grammar changed the transcript. Not shown in the UI. */
  raw_text?: string;
  corrected: boolean;
  /** A snippet expansion. Not shown in the UI — see SNIPPETS.md. */
  snippet: boolean;
  /** The vocabulary replaced something. Not shown in the UI — see VOCABULARY.md. */
  vocabulary: boolean;
  /**
   * The user's own correction, present only on rows they fixed. `text` keeps
   * meaning what was typed into the application, so the row shows this instead
   * when it is here.
   */
  edited?: string;
}

/**
 * Mirrors `vocabulary::Entry`. An empty `id` means "this is new".
 *
 * The entry is the term: a term with no variants is still worth having, and a
 * variant with no term is meaningless.
 */
export interface Term {
  id: string;
  /** Typed verbatim — `MongoDB`, `Next.js`. */
  term: string;
  /** What Whisper says instead, replaced deterministically. */
  variants: string[];
  source: "manual" | "learned";
}

/** Mirrors `snippets::Snippet`. An empty `id` means "this is new". */
export interface Snippet {
  id: string;
  trigger: string;
  content: string;
}

/** One correction: what was typed, and what the user made it. */
export interface Mapping {
  from: string;
  to: string;
}

/** Mirrors `learn::Suggestion` — a correction Piplo has seen enough of to ask about. */
export interface Suggestion extends Mapping {
  count: number;
}

/**
 * Mirrors `learn::Outcome` — what a correction did.
 *
 * `counted` is deliberately silent in the UI: the first two occurrences are quiet
 * by design.
 */
export type Outcome =
  | { kind: "nothing" }
  | { kind: "counted"; count: number }
  | { kind: "learned"; term: string; from: string; count: number };

/** Mirrors `settings::Settings`. */
export interface Settings {
  shortcut: string;
  grammarEnabled: boolean;
  widgetVisible: boolean;
  learnFromCorrections: boolean;
}

/**
 * What the webview is allowed to know about the Groq key.
 *
 * Deliberately not the key. It travels one way — in via `setApiKey`, never
 * back out — so the webview can show whether one is configured and which one,
 * without ever holding the secret. `hint` is the last four characters only.
 */
export interface ApiKeyStatus {
  configured: boolean;
  hint: string | null;
  /** `env` wins over a saved key and cannot be edited from the UI. */
  source: "env" | "settings" | "none";
}

export function getApiKeyStatus(): Promise<ApiKeyStatus> {
  return invoke("get_api_key_status");
}

/** Rejects with a message when the key is empty or the write fails. */
export function setApiKey(key: string): Promise<ApiKeyStatus> {
  return invoke("set_api_key", { key });
}

export function clearApiKey(): Promise<ApiKeyStatus> {
  return invoke("clear_api_key");
}

/** Widen the widget window for the pill, or narrow it back for the chip. */
export function widgetSetActive(active: boolean): Promise<void> {
  return invoke("widget_set_active", { active });
}

/** Remember where the window is, so the moves below can be offsets from it. */
export function widgetDragStart(): Promise<void> {
  return invoke("widget_drag_start");
}

/** Logical pixels moved since `widgetDragStart`. */
export function widgetDragTo(dx: number, dy: number): Promise<void> {
  return invoke("widget_drag_to", { dx, dy });
}

/** Pull the widget back on screen and remember where it landed. */
export function widgetDragEnd(): Promise<void> {
  return invoke("widget_drag_end");
}

/** Start a session from the mouse. A key-up will not end it. */
export function startDictation(): Promise<void> {
  return invoke("start_dictation");
}

/** ✓ — accept and transcribe. Only ends a session the mouse started. */
export function finishDictation(): Promise<void> {
  return invoke("finish_dictation");
}

/** ✕ — abort. Works whatever started the session, at any point in the flow. */
export function cancelDictation(): Promise<void> {
  return invoke("cancel_dictation");
}

/** Mirrors `shortcut::Binding` — one row of the read-only list in Settings. */
export interface Binding {
  accelerator: string;
  label: string;
  hint: string;
  /** False when the system refused the chord, so the page can say so. */
  bound: boolean;
}

/**
 * The fixed shortcuts, straight from the Rust constants that register them — so
 * the page cannot promise a chord that is not actually bound.
 */
export function listShortcuts(): Promise<Binding[]> {
  return invoke("list_shortcuts");
}

export function getSettings(): Promise<Settings> {
  return invoke("get_settings");
}

/**
 * Rejects with a message when the shortcut cannot be bound, leaving both the live
 * binding and the saved file untouched — so the caller must roll back its
 * optimistic update.
 */
export function setSettings(settings: Settings): Promise<Settings> {
  return invoke("set_settings", { settings });
}

/** Newest first. */
export function getHistory(): Promise<HistoryEntry[]> {
  return invoke("get_history");
}

/** Rewrites the log without that row. Rejects if it has already gone. */
export function deleteEntry(id: string): Promise<void> {
  return invoke("delete_entry", { id });
}

export function clearHistory(): Promise<void> {
  return invoke("clear_history");
}

/** File order — oldest first, since snippets are appended as they are created. */
export function listSnippets(): Promise<Snippet[]> {
  return invoke("list_snippets");
}

/**
 * Create and update both, keyed on `id`. Rejects with the message to show inside
 * the form — an empty trigger, empty content, or a trigger another snippet
 * already answers to.
 *
 * Returns the whole list, so the page replaces its state outright rather than
 * patching it and hoping the two stay in step.
 */
export function saveSnippet(snippet: Snippet): Promise<Snippet[]> {
  return invoke("save_snippet", { snippet });
}

export function deleteSnippet(id: string): Promise<Snippet[]> {
  return invoke("delete_snippet", { id });
}

/** File order — oldest first, since entries are appended as they are created. */
export function listVocabulary(): Promise<Term[]> {
  return invoke("list_vocabulary");
}

/**
 * Create and update both, keyed on `id`. Rejects with the message to show inside
 * the form — an empty term, a variant equal to its own term, a variant another
 * entry already claims, or a term that is already here.
 *
 * Returns the whole list, so the page replaces its state outright.
 */
export function saveTerm(entry: Term): Promise<Term[]> {
  return invoke("save_term", { entry });
}

export function deleteTerm(id: string): Promise<Term[]> {
  return invoke("delete_term", { id });
}

/**
 * Save the user's correction of one dictation, and learn from it.
 *
 * Rewrites that history line, then reports what it did — so the row can say so
 * where the correction was made, rather than through a notification layer.
 */
export function recordCorrection(id: string, edited: string): Promise<Outcome> {
  return invoke("record_correction", { id, edited });
}

/** Corrections worth asking about, and not already in the dictionary. */
export function listSuggestions(): Promise<Suggestion[]> {
  return invoke("list_suggestions");
}

/** *Add* — returns the whole dictionary, like every other mutation. */
export function acceptSuggestion(mapping: Mapping): Promise<Term[]> {
  return invoke("accept_suggestion", { mapping });
}

/**
 * *Delete* on a suggestion, and *Undo* on a row that just learned something.
 *
 * Forgets the pair — drops the variant and clears its count. Not a block: the same
 * correction happening again earns it back. Returns the remaining suggestions.
 */
export function rejectSuggestion(mapping: Mapping): Promise<Suggestion[]> {
  return invoke("reject_suggestion", { mapping });
}

export function openHome(): Promise<void> {
  return invoke("open_home");
}

export function showWidgetMenu(): Promise<void> {
  return invoke("show_widget_menu");
}

export function hideWidgetMenu(): Promise<void> {
  return invoke("hide_widget_menu");
}

export function quitApp(): Promise<void> {
  return invoke("quit_app");
}
