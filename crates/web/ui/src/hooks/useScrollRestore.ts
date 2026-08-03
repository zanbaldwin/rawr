import { useEffect } from "react";

/** Restore the document scroll position when returning to a view (the
 * windowed list makes this trivial: same data ⇒ same heights). */
export function useScrollRestore(key: string, ready: boolean): void {
  useEffect(() => {
    if (!ready) return;
    const stored = sessionStorage.getItem(`scroll:${key}`);
    if (stored) window.scrollTo(0, Number(stored));
    return () => {
      sessionStorage.setItem(`scroll:${key}`, String(window.scrollY));
    };
  }, [key, ready]);
}
