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
}

/** Mirrors `settings::Settings`. */
export interface Settings {
  shortcut: string;
  grammarEnabled: boolean;
  widgetVisible: boolean;
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

export function clearHistory(): Promise<void> {
  return invoke("clear_history");
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
