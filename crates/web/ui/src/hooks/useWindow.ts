// Document-scroll windowing over fixed-height rows (~45 lines instead of
// a virtualisation dependency). Scrolling the document keeps iOS
// momentum and position:sticky working.

import { useEffect, useRef, useState } from "react";

export interface WindowRange {
  ref: React.RefObject<HTMLDivElement | null>;
  start: number;
  end: number;
}

export function useWindow(count: number, rowHeight: number, overscan = 8): WindowRange {
  const ref = useRef<HTMLDivElement | null>(null);
  const [range, setRange] = useState<readonly [number, number]>([0, Math.min(count, 40)]);

  useEffect(() => {
    const element = ref.current;
    if (!element) return;
    let frame = 0;
    const update = () => {
      frame = 0;
      const top = -element.getBoundingClientRect().top;
      const start = Math.max(0, Math.floor(top / rowHeight) - overscan);
      const end = Math.min(count, Math.ceil((top + window.innerHeight) / rowHeight) + overscan);
      // Bail when unchanged — scroll fires far more often than the range moves.
      setRange((prev) => (prev[0] === start && prev[1] === end ? prev : [start, end]));
    };
    const onScroll = () => {
      if (!frame) frame = requestAnimationFrame(update);
    };
    update();
    window.addEventListener("scroll", onScroll, { passive: true });
    window.addEventListener("resize", onScroll);
    return () => {
      window.removeEventListener("scroll", onScroll);
      window.removeEventListener("resize", onScroll);
      if (frame) cancelAnimationFrame(frame);
    };
  }, [count, rowHeight, overscan]);

  return { ref, start: range[0], end: range[1] };
}

/** Row height comes from the `--row-h` CSS custom property — the one
 * cross-language invariant. Never duplicate the number in JS. */
export function useRowHeight(): number {
  const read = () =>
    Number.parseInt(getComputedStyle(document.documentElement).getPropertyValue("--row-h"), 10) || 96;
  const [height, setHeight] = useState(read);
  useEffect(() => {
    const media = window.matchMedia("(min-width: 700px)");
    const onChange = () => setHeight(read());
    media.addEventListener("change", onChange);
    return () => media.removeEventListener("change", onChange);
  }, []);
  return height;
}
