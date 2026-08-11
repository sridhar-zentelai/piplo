/**
 * Run with `bun test`.
 *
 * Deliberately outside `src/` — `tsconfig.json` has `"include": ["src"]`, so
 * keeping tests here means `bun run build` never tries to typecheck `bun:test`.
 */
import { describe, expect, it } from "bun:test";

import {
  blockedReason,
  chordFromEvent,
  DEFAULT_SHORTCUT,
  formatAccelerator,
  isSupportedCode,
  keyLabel,
  parseAccelerator,
  warningFor,
  type Chord,
  type KeyEventLike,
} from "../src/lib/shortcuts";

function parsed(input: string): Chord {
  const result = parseAccelerator(input);
  if (!result.ok) throw new Error(`expected "${input}" to parse: ${result.reason}`);
  return result.chord;
}

function reason(input: string): string {
  const result = parseAccelerator(input);
  if (result.ok) throw new Error(`expected "${input}" to fail`);
  return result.reason;
}

function keydown(code: string, mods: Partial<KeyEventLike> = {}): KeyEventLike {
  return {
    code,
    ctrlKey: false,
    altKey: false,
    shiftKey: false,
    metaKey: false,
    ...mods,
  };
}

describe("parseAccelerator", () => {
  it("parses the default", () => {
    expect(parsed(DEFAULT_SHORTCUT)).toEqual({ mods: ["Ctrl"], code: "Space" });
  });

  it("accepts both the bare letter and the code spelling", () => {
    expect(parsed("Alt+Shift+D")).toEqual({
      mods: ["Alt", "Shift"],
      code: "KeyD",
    });
    expect(parsed("Alt+Shift+KeyD")).toEqual(parsed("Alt+Shift+D"));
  });

  it("is case insensitive, like the Rust parser", () => {
    expect(parsed("cTRl+sPace")).toEqual(parsed("Ctrl+Space"));
    expect(parsed("CONTROL+shift+keyk")).toEqual(parsed("Ctrl+Shift+K"));
  });

  it("normalises modifier aliases", () => {
    expect(parsed("CmdOrCtrl+Space")).toEqual(parsed("Ctrl+Space"));
    expect(parsed("Option+F5")).toEqual({ mods: ["Alt"], code: "F5" });
    // Cmd/Command are the Windows key here, and survive parsing so that
    // blockedReason can explain them rather than the parser hiding them.
    expect(parsed("Cmd+D").mods).toEqual(["Super"]);
  });

  it("orders modifiers canonically regardless of input order", () => {
    expect(parsed("Shift+Alt+Ctrl+D").mods).toEqual(["Ctrl", "Alt", "Shift"]);
  });

  it("tolerates a repeated modifier", () => {
    expect(parsed("Ctrl+Ctrl+D").mods).toEqual(["Ctrl"]);
  });

  it("resolves punctuation and numpad aliases", () => {
    expect(parsed("Ctrl+/")).toEqual({ mods: ["Ctrl"], code: "Slash" });
    expect(parsed("Ctrl+,")).toEqual({ mods: ["Ctrl"], code: "Comma" });
    expect(parsed("Ctrl+Num5")).toEqual({ mods: ["Ctrl"], code: "Numpad5" });
    expect(parsed("Ctrl+esc")).toEqual({ mods: ["Ctrl"], code: "Escape" });
    expect(parsed("Ctrl+Up")).toEqual({ mods: ["Ctrl"], code: "ArrowUp" });
  });

  it("rejects a key placed before a modifier", () => {
    expect(reason("Ctrl+D+Shift")).toContain("before a modifier");
    expect(reason("Ctrl+Shift+C+A")).toContain("before a modifier");
  });

  it("rejects empty parts", () => {
    expect(reason("Ctrl++")).toContain("empty part");
    expect(reason("")).toBe("No shortcut set.");
  });

  it("rejects modifiers with no key", () => {
    expect(reason("Ctrl+Shift")).toContain("no key");
  });

  it("rejects keys the binder cannot express", () => {
    // Real event.code values that global-hotkey's parse_key has no arm for.
    for (const code of ["IntlBackslash", "ContextMenu", "NumpadComma", "F25", "Lang1"]) {
      expect(reason(`Ctrl+${code}`)).toContain("is not a key");
    }
  });

  it("parses a bare key, leaving the policy to blockedReason", () => {
    expect(parsed("Space")).toEqual({ mods: [], code: "Space" });
  });
});

describe("formatAccelerator", () => {
  it("round-trips", () => {
    for (const input of [
      "Ctrl+Space",
      "Alt+Shift+D",
      "Ctrl+Alt+Shift+F12",
      "Ctrl+Slash",
      "Ctrl+ArrowUp",
      "Ctrl+Numpad5",
      "Ctrl+NumpadAdd",
    ]) {
      expect(formatAccelerator(parsed(input))).toBe(input);
    }
  });

  it("writes the short form for letters and digits", () => {
    expect(formatAccelerator({ mods: ["Ctrl"], code: "KeyD" })).toBe("Ctrl+D");
    expect(formatAccelerator({ mods: ["Ctrl"], code: "Digit5" })).toBe("Ctrl+5");
  });

  it("canonicalises a hand-edited settings value", () => {
    expect(formatAccelerator(parsed("shift+control+keyk"))).toBe("Ctrl+Shift+K");
  });

  it("never emits a token containing the separator", () => {
    // "+" as a key would make the string unsplittable; Equal is the way out.
    expect(formatAccelerator({ mods: ["Ctrl"], code: "Equal" })).toBe("Ctrl+Equal");
  });
});

describe("chordFromEvent", () => {
  it("waits while only modifiers are held", () => {
    for (const code of ["ControlLeft", "AltRight", "ShiftLeft", "MetaLeft"]) {
      expect(chordFromEvent(keydown(code, { ctrlKey: true }))).toEqual({
        status: "incomplete",
      });
    }
  });

  it("reads code, not key — Ctrl+Shift+1 is a digit, not '!'", () => {
    const capture = chordFromEvent(
      keydown("Digit1", { ctrlKey: true, shiftKey: true }),
    );
    expect(capture.status).toBe("chord");
    if (capture.status !== "chord") return;
    expect(formatAccelerator(capture.chord)).toBe("Ctrl+Shift+1");
  });

  it("captures the Windows key so it can be explained, not dropped", () => {
    const capture = chordFromEvent(keydown("KeyD", { ctrlKey: true, metaKey: true }));
    expect(capture.status).toBe("chord");
    if (capture.status !== "chord") return;
    expect(capture.chord.mods).toEqual(["Ctrl", "Super"]);
    expect(blockedReason(capture.chord)).toContain("Windows key");
  });

  it("reports unbindable keys instead of pretending they worked", () => {
    expect(chordFromEvent(keydown("IntlBackslash", { ctrlKey: true }))).toEqual({
      status: "unsupported",
      code: "IntlBackslash",
    });
  });

  it("produces something formatAccelerator can hand back to Rust", () => {
    const capture = chordFromEvent(keydown("KeyD", { altKey: true, shiftKey: true }));
    if (capture.status !== "chord") throw new Error("expected a chord");
    const accelerator = formatAccelerator(capture.chord);
    expect(accelerator).toBe("Alt+Shift+D");
    expect(parsed(accelerator)).toEqual(capture.chord);
  });
});

describe("blockedReason", () => {
  it("refuses the Windows key in any combination", () => {
    expect(blockedReason(parsed("Super+D"))).toContain("Windows key");
    expect(blockedReason(parsed("Ctrl+Super+ArrowLeft"))).toContain("Windows key");
    expect(blockedReason(parsed("Cmd+Shift+D"))).toContain("Windows key");
  });

  it("requires at least one modifier", () => {
    expect(blockedReason(parsed("Space"))).toContain("Ctrl, Alt or Shift");
    expect(blockedReason(parsed("F5"))).toContain("Ctrl, Alt or Shift");
  });

  it("allows any combination of Ctrl, Alt and Shift", () => {
    for (const input of [
      "Ctrl+Space",
      "Alt+Shift+D",
      "Shift+F9",
      "Ctrl+Alt+Shift+K",
    ]) {
      expect(blockedReason(parsed(input))).toBeNull();
    }
  });

  it("clears the default", () => {
    expect(blockedReason(parsed(DEFAULT_SHORTCUT))).toBeNull();
  });
});

describe("warningFor", () => {
  it("warns about AltGr on Ctrl+Alt", () => {
    expect(warningFor("Ctrl+Alt+D")).toContain("AltGr");
    expect(warningFor("Ctrl+Alt+Shift+D")).toContain("AltGr");
  });

  it("stays quiet otherwise", () => {
    expect(warningFor("Ctrl+Space")).toBeNull();
    expect(warningFor("Alt+Shift+D")).toBeNull();
  });

  it("works off the saved string, including a hand-edited one", () => {
    expect(warningFor("control+option+keyd")).toContain("AltGr");
  });

  it("says nothing about a value it cannot parse", () => {
    expect(warningFor("Ctrl+Nonsense")).toBeNull();
    expect(warningFor("")).toBeNull();
  });
});

describe("keyLabel / isSupportedCode", () => {
  it("shortens letters and digits only", () => {
    expect(keyLabel("KeyD")).toBe("D");
    expect(keyLabel("Digit5")).toBe("5");
    expect(keyLabel("Space")).toBe("Space");
    expect(keyLabel("Numpad5")).toBe("Numpad5");
    expect(keyLabel("F12")).toBe("F12");
  });

  it("knows what the binder supports", () => {
    expect(isSupportedCode("KeyD")).toBe(true);
    expect(isSupportedCode("F24")).toBe(true);
    expect(isSupportedCode("AudioVolumeMute")).toBe(true);
    expect(isSupportedCode("ContextMenu")).toBe(false);
    expect(isSupportedCode("MetaLeft")).toBe(false);
  });
});
