import { useCallback, useRef } from "react";
import { widgetDragEnd, widgetDragStart, widgetDragTo } from "@/lib/commands";

/** How far the pointer has to travel before a press is a drag and not a click.
 *  Small enough that dragging feels immediate, large enough that an unsteady
 *  click on the mic still dictates. */
const THRESHOLD = 4;

/**
 * Drag the widget window by pressing anywhere on it.
 *
 * Screen coordinates, not client ones: the window moves out from under the
 * cursor as the drag goes, so a client-relative delta would fight itself.
 */
export function useWidgetDrag() {
  const origin = useRef<{ x: number; y: number } | null>(null);
  const dragging = useRef(false);
  const offset = useRef({ dx: 0, dy: 0 });
  const frame = useRef<number | null>(null);

  // Commands are chained rather than fired loose: the moves have to reach Rust
  // after the start that gives them an origin, and before the drop that writes
  // the position down.
  const queue = useRef<Promise<unknown>>(Promise.resolve());
  const send = useCallback((task: () => Promise<unknown>) => {
    queue.current = queue.current.then(task, task);
  }, []);

  // pointermove outruns the IPC round trip and only the latest offset matters,
  // so a frame's worth of moves collapses into one.
  const flush = useCallback(() => {
    frame.current = null;
    send(() => widgetDragTo(offset.current.dx, offset.current.dy));
  }, [send]);

  const stop = useCallback(
    (event: React.PointerEvent) => {
      origin.current = null;

      if (event.currentTarget.hasPointerCapture(event.pointerId)) {
        event.currentTarget.releasePointerCapture(event.pointerId);
      }

      if (!dragging.current) return;

      if (frame.current !== null) {
        cancelAnimationFrame(frame.current);
        frame.current = null;
      }

      // The last move is worth more than the frame it would have waited for.
      flush();
      send(widgetDragEnd);
    },
    [flush, send],
  );

  return {
    /** True for the click that ends a drag, so it does not also start a
     *  dictation. Cleared by the next press. */
    dragged: () => dragging.current,

    handlers: {
      onPointerDown: (event: React.PointerEvent) => {
        if (event.button !== 0) return;
        origin.current = { x: event.screenX, y: event.screenY };
        dragging.current = false;
      },

      onPointerMove: (event: React.PointerEvent) => {
        if (!origin.current) return;

        // A release outside the window before the drag began never reached us.
        if (event.buttons === 0) {
          origin.current = null;
          return;
        }

        const dx = event.screenX - origin.current.x;
        const dy = event.screenY - origin.current.y;

        if (!dragging.current) {
          if (Math.hypot(dx, dy) < THRESHOLD) return;

          dragging.current = true;
          // Captured only once the drag is real: capturing on the press would
          // retarget the click away from the mic button and swallow every one.
          event.currentTarget.setPointerCapture(event.pointerId);
          send(widgetDragStart);
        }

        offset.current = { dx, dy };
        frame.current ??= requestAnimationFrame(flush);
      },

      onPointerUp: stop,
      onPointerCancel: stop,
    },
  };
}
