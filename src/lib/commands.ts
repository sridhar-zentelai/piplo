import { invoke } from "@tauri-apps/api/core";

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
