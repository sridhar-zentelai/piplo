/**
 * Accelerator parsing and validation for the push-to-talk shortcut.
 *
 * This mirrors `global-hotkey`'s `parse_hotkey` (the crate behind
 * `tauri-plugin-global-shortcut`) on purpose: what this module accepts is what
 * Rust will bind, so the recorder cannot offer a chord that fails on save.
 * Its token table is the source of truth — see `parse_key` in
 * global-hotkey/src/hotkey.rs.
 *
 * Syntax lives in `parseAccelerator`, policy lives in `blockedReason` and
 * `warningFor`. They are split because `settings.json` is hand-editable: a
 * chord can be perfectly well-formed and still be one we refuse to bind.
 */

export type Modifier = "Ctrl" | "Alt" | "Shift" | "Super";

export interface Chord {
  /** Deduplicated, always in `MOD_ORDER`. */
  mods: Modifier[];
  /** A `KeyboardEvent.code` value, e.g. `"KeyD"`, `"Space"`, `"ArrowUp"`. */
  code: string;
}

export type ParseResult =
  | { ok: true; chord: Chord }
  | { ok: false; reason: string };

/** What the recorder gets back from a keydown. */
export type Capture =
  /** Only modifiers are down so far — keep waiting. */
  | { status: "incomplete" }
  /** A real key, but not one the binder can express. */
  | { status: "unsupported"; code: string }
  | { status: "chord"; chord: Chord };

export const DEFAULT_SHORTCUT = "Ctrl+Space";

/** Ctrl first so chords read the way Windows writes them. */
const MOD_ORDER: Modifier[] = ["Ctrl", "Alt", "Shift", "Super"];

/**
 * Modifier spellings `parse_hotkey` accepts, uppercased.
 * `CommandOrControl` resolves to Ctrl here because Piplo is Windows-only.
 */
const MODIFIER_TOKENS: Record<string, Modifier> = {
  CTRL: "Ctrl",
  CONTROL: "Ctrl",
  COMMANDORCONTROL: "Ctrl",
  COMMANDORCTRL: "Ctrl",
  CMDORCTRL: "Ctrl",
  CMDORCONTROL: "Ctrl",
  ALT: "Alt",
  OPTION: "Alt",
  SHIFT: "Shift",
  SUPER: "Super",
  COMMAND: "Super",
  CMD: "Super",
};

const LETTERS = "ABCDEFGHIJKLMNOPQRSTUVWXYZ".split("");
const DIGITS = "0123456789".split("");

/**
 * Every `KeyboardEvent.code` the Rust parser can express, in canonical
 * spelling. Codes absent here — `IntlBackslash`, `ContextMenu`, `NumpadComma`,
 * `F25`, the `Lang*` keys — are real `event.code` values that `parse_key`
 * rejects, so we have to refuse them at capture time rather than at save time.
 */
const SUPPORTED_CODES: string[] = [
  ...LETTERS.map((l) => `Key${l}`),
  ...DIGITS.map((d) => `Digit${d}`),
  "Backquote",
  "Backslash",
  "BracketLeft",
  "BracketRight",
  "Comma",
  "Equal",
  "Minus",
  "Period",
  "Quote",
  "Semicolon",
  "Slash",
  "Backspace",
  "CapsLock",
  "Enter",
  "Space",
  "Tab",
  "Delete",
  "End",
  "Home",
  "Insert",
  "PageDown",
  "PageUp",
  "PrintScreen",
  "ScrollLock",
  "Pause",
  "NumLock",
  "ArrowDown",
  "ArrowLeft",
  "ArrowRight",
  "ArrowUp",
  "Escape",
  ...DIGITS.map((d) => `Numpad${d}`),
  "NumpadAdd",
  "NumpadDecimal",
  "NumpadDivide",
  "NumpadEnter",
  "NumpadEqual",
  "NumpadMultiply",
  "NumpadSubtract",
  ...Array.from({ length: 24 }, (_, i) => `F${i + 1}`),
  "AudioVolumeDown",
  "AudioVolumeUp",
  "AudioVolumeMute",
  "MediaPlay",
  "MediaPause",
  "MediaPlayPause",
  "MediaStop",
  "MediaTrackNext",
  "MediaTrackPrevious",
];

const SUPPORTED = new Set(SUPPORTED_CODES);

/**
 * Extra spellings `parse_key` accepts, beyond the canonical code name.
 * A hand-edited `"Ctrl+esc"` or `"Ctrl+/"` should load and then render as the
 * canonical form rather than being thrown away.
 */
const CODE_ALIASES: Record<string, string> = {
  "`": "Backquote",
  "\\": "Backslash",
  "[": "BracketLeft",
  "]": "BracketRight",
  ",": "Comma",
  "=": "Equal",
  "-": "Minus",
  ".": "Period",
  "'": "Quote",
  ";": "Semicolon",
  "/": "Slash",
  ESC: "Escape",
  PAUSEBREAK: "Pause",
  DOWN: "ArrowDown",
  LEFT: "ArrowLeft",
  RIGHT: "ArrowRight",
  UP: "ArrowUp",
  NUMADD: "NumpadAdd",
  NUMPADPLUS: "NumpadAdd",
  NUMPLUS: "NumpadAdd",
  NUMDECIMAL: "NumpadDecimal",
  NUMDIVIDE: "NumpadDivide",
  NUMENTER: "NumpadEnter",
  NUMEQUAL: "NumpadEqual",
  NUMMULTIPLY: "NumpadMultiply",
  NUMSUBTRACT: "NumpadSubtract",
  VOLUMEDOWN: "AudioVolumeDown",
  VOLUMEUP: "AudioVolumeUp",
  VOLUMEMUTE: "AudioVolumeMute",
  MEDIATRACKPREV: "MediaTrackPrevious",
};

/** Uppercased token -> canonical `event.code`. */
const TOKEN_TO_CODE = new Map<string, string>();
for (const code of SUPPORTED_CODES) TOKEN_TO_CODE.set(code.toUpperCase(), code);
// Bare letters and digits: the docs write `Alt+Shift+D`, not `Alt+Shift+KeyD`.
for (const l of LETTERS) TOKEN_TO_CODE.set(l, `Key${l}`);
for (const d of DIGITS) TOKEN_TO_CODE.set(d, `Digit${d}`);
for (const d of DIGITS) TOKEN_TO_CODE.set(`NUM${d}`, `Numpad${d}`);
for (const [alias, code] of Object.entries(CODE_ALIASES)) {
  TOKEN_TO_CODE.set(alias.toUpperCase(), code);
}

/** `"KeyD"` -> `"D"`, `"Digit5"` -> `"5"`; everything else keeps its name. */
export function keyLabel(code: string): string {
  if (/^Key[A-Z]$/.test(code)) return code.slice(3);
  if (/^Digit[0-9]$/.test(code)) return code.slice(5);
  return code;
}

export function isSupportedCode(code: string): boolean {
  return SUPPORTED.has(code);
}

function sortMods(mods: Iterable<Modifier>): Modifier[] {
  const present = new Set(mods);
  return MOD_ORDER.filter((m) => present.has(m));
}

/**
 * Parse an accelerator string the way Rust will. Syntax only — a result of
 * `ok` does not mean we are willing to bind it, see `blockedReason`.
 */
export function parseAccelerator(input: string): ParseResult {
  if (input.trim() === "") return { ok: false, reason: "No shortcut set." };

  const tokens = input.split("+");
  const mods = new Set<Modifier>();
  let code: string | null = null;

  for (const raw of tokens) {
    const token = raw.trim();
    if (token === "") {
      return { ok: false, reason: `"${input}" has an empty part.` };
    }

    const upper = token.toUpperCase();
    const modifier = MODIFIER_TOKENS[upper];

    // Rust fails on anything after the key, so "Ctrl+D+Shift" is not a chord
    // written out of order — it is not a chord at all.
    if (code !== null) {
      return { ok: false, reason: `"${input}" puts the key before a modifier.` };
    }

    if (modifier) {
      mods.add(modifier);
      continue;
    }

    const resolved = TOKEN_TO_CODE.get(upper);
    if (!resolved) return { ok: false, reason: `"${token}" is not a key.` };
    code = resolved;
  }

  if (code === null) return { ok: false, reason: `"${input}" has no key.` };
  return { ok: true, chord: { mods: sortMods(mods), code } };
}

/** The storage form, and what Rust gets handed to bind. */
export function formatAccelerator(chord: Chord): string {
  return [...sortMods(chord.mods), keyLabel(chord.code)].join("+");
}

/**
 * The subset of `KeyboardEvent` this needs — structural so it can be exercised
 * without a DOM.
 */
export interface KeyEventLike {
  code: string;
  ctrlKey: boolean;
  altKey: boolean;
  shiftKey: boolean;
  metaKey: boolean;
}

/** Codes that only ever contribute a modifier, never the key itself. */
const MODIFIER_CODES = new Set([
  "ControlLeft",
  "ControlRight",
  "AltLeft",
  "AltRight",
  "ShiftLeft",
  "ShiftRight",
  "MetaLeft",
  "MetaRight",
]);

/**
 * Build a chord from a keydown.
 *
 * Reads `event.code`, never `event.key`: `key` reports the *result* of the
 * chord, so `Ctrl+Shift+1` arrives as `"!"` — not a key anyone can press alone,
 * and not a token the binder accepts.
 */
export function chordFromEvent(event: KeyEventLike): Capture {
  if (MODIFIER_CODES.has(event.code)) return { status: "incomplete" };
  if (!isSupportedCode(event.code)) {
    return { status: "unsupported", code: event.code };
  }

  const mods = new Set<Modifier>();
  if (event.ctrlKey) mods.add("Ctrl");
  if (event.altKey) mods.add("Alt");
  if (event.shiftKey) mods.add("Shift");
  if (event.metaKey) mods.add("Super");

  return { status: "chord", chord: { mods: sortMods(mods), code: event.code } };
}

/**
 * Why this chord cannot be bound, or `null` if it can.
 *
 * The Windows key is refused outright. `RegisterHotKey` reports success for
 * combinations the shell has already claimed — `Ctrl+Win+D` and friends — and
 * then never fires, so the shortcut looks bound and silently does nothing,
 * which is worse than being told no. The reserved set is undocumented and grows
 * with each Windows release, so allowing "the rest" would be guesswork.
 */
export function blockedReason(chord: Chord): string | null {
  if (chord.mods.includes("Super")) {
    return "The Windows key can't be used — Windows reserves these combinations and would swallow the shortcut without telling you.";
  }
  if (chord.mods.length === 0) {
    return "Add Ctrl, Alt or Shift — a key on its own would fire in every app as you typed.";
  }
  return null;
}

/**
 * A caveat worth showing, or `null`. Takes the saved accelerator rather than a
 * freshly recorded chord so it also appears for a binding carried over from an
 * older build.
 */
export function warningFor(accelerator: string): string | null {
  const parsed = parseAccelerator(accelerator);
  if (!parsed.ok) return null;

  // Windows treats Ctrl+Alt as AltGr, so binding it takes over every AltGr
  // character (€, most accented letters) in every app. Only layouts that have
  // the key are affected, so this is a warning and not a refusal.
  const { mods } = parsed.chord;
  if (mods.includes("Ctrl") && mods.includes("Alt")) {
    return "Ctrl+Alt is AltGr on many keyboard layouts. Binding it will take over the accented characters typed with AltGr.";
  }
  return null;
}
