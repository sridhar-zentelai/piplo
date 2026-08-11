# Settings

**Three settings.** On the home window, applied immediately, no restart.

| Setting | Default | Effect |
| ------- | ------- | ------ |
| **Shortcut** | `Ctrl+Space` | The global push-to-talk accelerator |
| **Automatic grammar correction** | On | Whether transcripts are cleaned up before typing |
| **Show floating widget** | On | Whether the chip sits on screen while idle. Off still leaves the shortcut working — the pill appears for the session and goes away again |

That is the whole list. No device picker, no theme, no launch-at-login, no model
picker, no language override. Each of those is a real feature request and each
one is out of [scope](../CLAUDE.md#do-not-implement).

Implemented in `src-tauri/src/settings.rs` and `src/components/SettingsPanel.tsx`.

---

## Where they live

`settings.json` in the platform config dir — `%APPDATA%\com.codea.piplo` on
Windows. **Not in the repo.**

```json
{
  "shortcut": "Ctrl+Space",
  "grammarEnabled": true,
  "widgetVisible": true
}
```

The split from history is deliberate: history is meant to be read and grepped, so
it sits in the checkout. Settings are machine state that should follow the
install, not the clone.

Every field gets a `serde` default, so a file written by an older build loads
what it has rather than falling back wholesale and losing the user's shortcut.

A corrupt or half-written file is **ignored in favour of the defaults** rather
than failing the launch. The next save overwrites it. A settings file must never
be able to prevent the app from starting.

---

## Applying without a restart

### The shortcut

Re-registered live in `shortcut::rebind`. **The order matters:**

1. Register the new accelerator.
2. Only once that succeeds, unregister the old one.

The other way round means an accelerator already claimed by another app leaves
Piplo with **nothing bound** — no way to dictate, and no obvious way to recover.

`set_settings` follows the same principle end to end: rebind, then update state,
then write the file. A rejected shortcut leaves the live binding *and* the saved
file untouched, and the UI rolls its optimistic update back with the error
message.

At startup, `register_initial` falls back to `Ctrl+Space` if the saved shortcut
no longer parses or has since been claimed by something else. **A saved setting
should never be able to brick the app.**

### The grammar toggle

Read from settings on every dictation, inside `session::deliver`, so flipping it
applies to the very next one. `PIPLO_GRAMMAR=0` remains a forced override for
debugging — see [GRAMMAR.md](GRAMMAR.md#controls).

Also togglable from the [right-click menu](WIDGET.md#right-click-menu). Both
surfaces read through `get_settings` rather than caching, so neither can show a
stale switch.

### The widget toggle

Off hides the chip via `widget::hide` and sets a flag that `session` respects:
the pill still appears for the duration of a dictation, then goes away again.

Turning the widget off must not turn dictation off. It is a "get out of my
screen" control, not a kill switch — and if it disabled the shortcut too, a user
who set it would have no way to dictate and no visible UI to fix it from.

---

## Recording a shortcut

`ShortcutRecorder.tsx` captures a real chord rather than accepting free text, so
an unparseable accelerator cannot be typed in the first place.

Two details worth keeping:

- **It reads `event.code`, not `event.key`.** `key` reports the *result* of the
  chord — `Ctrl+Shift+1` arrives as `"!"`, which is not a key anyone can press on
  its own.
- **At least one modifier is required.** A bare letter would fire in every app
  the moment the user typed it.

`Escape` leaves the field without binding anything.

---

## Which modifiers are allowed

Split between a hard rule and a warning, in `src/lib/shortcuts.ts`:

| Chord | Verdict |
| ----- | ------- |
| `Ctrl` / `Alt` / `Shift`, any combination | Allowed |
| Anything with the **Windows key** | **Refused** |
| `Ctrl+Alt+…` | Allowed, with a warning |

**The Windows key is refused outright.** Windows reserves a large set of `Win`
combinations — `Ctrl+Win+←` switches virtual desktop, `Ctrl+Win+D` creates one —
and `RegisterHotKey` reports **success** for them regardless. The shell takes the
keypress first, so the shortcut looks bound and silently never fires, which is
worse than being told no. The reserved set is undocumented and grows with each
Windows release, so allowing "the rest" would be guesswork.

Enforced twice on purpose: `blockedReason` in the recorder for immediate
feedback, and `shortcut::parse` in Rust because `settings.json` is a plain file
and can be edited by hand. A `Super` binding that arrives that way fails to
register at startup, and `register_initial` falls back to `Ctrl+Space`.

**`Ctrl+Alt` is allowed but warned about**, because it *is* AltGr as far as
Windows is concerned — binding it takes over every AltGr character (€ on
`AltGr+E`, most accented letters) in every app. Only layouts that have the key
are affected, so refusing it would penalise US-layout users for a problem they do
not have. Derive the warning from the saved value rather than from the moment it
was recorded, so it also appears for a shortcut carried over from an older build.

---

## Known limits

- **No conflict detection against other apps** beyond what Windows reports when
  registration fails. Windows will happily let some combinations register and
  then be swallowed by whatever grabbed them first. Refusing the Windows key
  removes the worst offenders, but a third-party app that grabbed, say,
  `Ctrl+Shift+K` first still wins silently.
- **`Ctrl+Space` collides with the Windows IME** for CJK input — which is the
  main reason this setting exists at all.
- **No reset-to-defaults button.** Deleting `settings.json` does the same thing.
- The shortcut cannot be captured while the widget is recording. The home window
  is a separate window and unaffected, but changing the binding mid-take is
  untested.
