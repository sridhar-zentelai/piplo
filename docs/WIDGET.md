# The Floating Widget

A small chip that starts bottom-centre of the screen, can be dragged anywhere,
and morphs into a recording pill while you dictate. It is the entire visible
surface of Piplo most of the time.

```
   idle              recording                transcribing
 ┌──────┐      ┌────────────────────┐      ┌────────────────────┐
 │  ◉   │  →   │ ✕  ▁▃▅▂▇▃▁  0:04 ✓ │  →   │ ✕     ● ● ●        │
 └──────┘      └────────────────────┘      └────────────────────┘
  56×56                ~260×56                    ~260×56
```

Same shape for `recording` and `transcribing` on purpose — no extra morph
mid-flight, so the two network calls stay invisible.

---

## Window properties

```json
{
  "label": "widget",
  "width": 56, "height": 56,
  "decorations": false,
  "transparent": true,
  "alwaysOnTop": true,
  "visible": true,
  "resizable": false,
  "maximizable": false, "minimizable": false,
  "skipTaskbar": true,
  "shadow": false,
  "focus": false
}
```

`shadow: false` matters with `transparent: true` — Windows draws the shadow
around the *window* rectangle, not the rounded content, so you get a visible
square halo around a round chip.

## Why it never takes focus

`platform.rs` sets `WS_EX_NOACTIVATE` on the widget's HWND:

```rust
let ex = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
SetWindowLongPtrW(hwnd, GWL_EXSTYLE, ex | WS_EX_NOACTIVATE.0 as isize);
```

This is the load-bearing detail of the whole app. The text is typed into
"whatever window has focus" — so if showing the pill or clicking the mic button
moved focus to Piplo, the text would be typed into Piplo. `WS_EX_NOACTIVATE`
means clicks are delivered to the window without activating it.

Everything downstream depends on it, which is why it is verified in
[M1](MVP_PLAN.md#m1--shell) before a single keystroke is ever synthesised.

`focus: false` in the config alone is not enough — it governs the initial show,
not subsequent clicks.

## Position

Bottom-centre of the **work area**, not the screen: `MonitorFromWindow` →
`GetMonitorInfoW` → `rcWork`, which excludes the taskbar. Using `rcMonitor`
puts the chip behind the taskbar on the default Windows setup.

Resizing to the pill keeps the centre fixed, so the widget grows outward in both
directions instead of appearing to slide.

That is only the **default** home. Press anywhere on the widget and drag, and it
stays where it is dropped.

The drop point is stored as the widget's **centre**, in physical screen
coordinates, in `widget.json` beside `settings.json` — its own file, because it
is the residue of a mouse gesture rather than a fourth setting. The centre so the
pill still grows both ways from it; physical because the value is only ever
compared against monitor rectangles, and a round trip through the scale factor of
whichever monitor the window starts on drifts on a mixed-DPI desktop.

Placement is always clamped into the work area, so a widget can never be dropped,
restored, or resized somewhere the mouse cannot reach it. Individual moves are
**not** clamped: clamping each one against the current monitor would trap the
widget on it, because Windows only hands a window to the next monitor once it is
already mostly there. The drop clamps instead.

A `Moved` event that is not part of a drag — a resolution change, which reaches
Tauri as nothing else — only pulls the widget back on screen. It does not
re-home it, or a deliberate position would be undone by a monitor being
unplugged.

---

## States

| Status | Contents | Ring |
| ------ | -------- | ---- |
| `idle` | Mic glyph. Dimmed until hovered | none |
| `recording` | ✕ · waveform · elapsed time · ✓ | accent `#7C5CFF` |
| `transcribing` | ✕ · pulsing dots (waveform hidden) | accent, pulsing |
| `error` | ✕ · short message | error `#FF5C5C`, ~2.5 s |

**✓ is hidden while transcribing** — there is nothing left to accept. **✕ stays
live** as an abort through the whole flow, including during both network calls.

## Components

| File | Responsibility |
| ---- | -------------- |
| `FloatingWidget.tsx` | The shape map and the morph. Nothing else |
| `PillContents.tsx` | What renders inside, per status |
| `Waveform.tsx` | Bars from `level`. The only file that changes when audio behaviour changes |
| `MicIcon.tsx` | The glyph |

The shape map keeps the morph declarative:

```ts
const SHAPE = {
  idle:         { width: 56,  height: 56, borderRadius: 28 },
  recording:    { width: 260, height: 56, borderRadius: 28 },
  transcribing: { width: 260, height: 56, borderRadius: 28 },
  error:        { width: 260, height: 56, borderRadius: 28 },
} as const
```

`transcribing` and `error` intentionally reuse the recording geometry. Adding an
entry with different dimensions would introduce a morph the user reads as a
glitch.

## The waveform

`level` arrives ~30×/second as a single `f32` — one RMS value, not a spectrum.
`Waveform.tsx` turns it into bars by keeping a short rolling history and letting
each bar decay toward zero rather than snapping:

```ts
next = Math.max(incoming, previous * 0.86)
```

Without the decay the bars flicker between frames and read as noise. With it
they fall off the way a level meter does.

Bar heights are clamped to a minimum so the pill never looks broken in silence.

## The mic button

Click to start, then ✓ to finish or ✕ to cancel — a full mouse path with no
keyboard. Its session must **not** be ended by a `Ctrl+Space` key-up event; the
session records how it was started and only the matching trigger can finish it.
Without that, letting go of a key you happened to be holding kills a
mouse-started take.

Because the widget also drags, a press is only a click if it goes nowhere.
`useWidgetDrag` waits for 4px of travel before it starts moving the window, and
swallows the click that ends a drag — so the mic button under the pointer still
works, and dragging by the mic does not start a dictation.

Pointer capture is taken at that 4px mark and not at the press: capturing on the
press retargets the click away from the button and swallows every one of them.

The drag is driven by `screenX`/`screenY` deltas, not client ones — the window
moves out from under the cursor as it goes, so a client-relative delta fights
itself.

---

# Right-click menu

A separate `menu` window rather than a native context menu, so it matches the
widget's look and can host a toggle switch.

```
 ┌───────────────────────────┐
 │  ◉  Dictate               │
 │  ✓  Grammar correction  ● │
 │ ───────────────────────── │
 │  ⌂  Open Piplo            │
 │  ⏻  Quit                  │
 └───────────────────────────┘
```

Four items. Anything else belongs on the settings page.

| Item | Action |
| ---- | ------ |
| **Dictate** | `start_dictation`, then dismiss. ✓/✕ on the pill finish it |
| **Grammar correction** | Toggles the setting live, in place. The menu stays open |
| **Open Piplo** | Shows and focuses the `home` window, dismisses |
| **Quit** | Exits |

## Window

Same treatment as the widget: frameless, transparent, always on top,
`skipTaskbar`, `shadow: false`, and **`WS_EX_NOACTIVATE`**. The Notepad focus
test applies here too — opening the menu must not move focus.

## Positioning

Placed adjacent to the widget, then clamped into the work area so it never opens
off-screen when the widget is near an edge. Compute against `rcWork` of the
monitor the widget is on, not the primary monitor.

## Dismissal

Three ways, all of which must work:

- Clicking an item that isn't the toggle
- `Escape`
- **Clicking anywhere outside** — the tricky one. `WS_EX_NOACTIVATE` means the
  window never gets focus, so there is no blur event to listen for. Use the
  window's `Focused(false)` event where available, plus a low-frequency poll of
  the foreground window, and hide on either.

Do **not** solve this with a full-screen transparent overlay window to catch the
click. It swallows the first click intended for the app underneath, which is the
click the user actually wanted.

## Keeping the toggle honest

The menu reads `get_settings` each time it is shown rather than caching, and
writes through `set_settings`. Both the menu and the settings page are views of
the same file — a cached copy in either one shows a stale switch position, which
is worse than a brief flicker while it loads.
