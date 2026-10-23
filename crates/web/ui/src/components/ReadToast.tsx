// The "marked as read" toast with its Undo button. App-level, so it
// survives navigating away from the work straight after marking it.

import { useEffect } from "react";
import { reads } from "../data/instance";
import { isoToDays } from "../data/reads";
import { formatDay } from "../format";
import { useStore } from "../hooks/useStore";
import { createStore } from "../store";
import type { ReadEventDto } from "../types/generated/ReadEventDto";

interface ToastState {
  event: ReadEventDto | null;
  error: string | null;
}

const UNDO_WINDOW_MS = 8_000;

const toastStore = createStore<ToastState>({ event: null, error: null });

export function showReadToast(event: ReadEventDto): void {
  toastStore.set({ event, error: null });
}

export function ReadToast() {
  const { event, error } = useStore(toastStore);

  useEffect(() => {
    if (event === null) return;
    const timer = setTimeout(() => toastStore.set({ event: null, error: null }), UNDO_WINDOW_MS);
    return () => clearTimeout(timer);
  }, [event, error]);

  if (event === null) return null;

  const undo = async () => {
    try {
      await reads.undo(event);
      toastStore.set({ event: null, error: null });
    } catch (e) {
      toastStore.set({ event, error: e instanceof Error ? e.message : String(e) });
    }
  };

  const what = event.chapter_update ? "New chapter marked as read" : "Marked as read";
  return (
    <div className="read-toast" role="status">
      <span>{error ? `Undo failed: ${error}` : `${what} on ${formatDay(isoToDays(event.date))}.`}</span>
      <button type="button" onClick={() => void undo()}>
        {error ? "Try again" : "Undo"}
      </button>
    </div>
  );
}
