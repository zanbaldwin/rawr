import { useState } from "react";
import { reads } from "../data/instance";
import { localToday } from "../data/reads";
import { useSyncState } from "../hooks/useSync";
import { showReadToast } from "./ReadToast";

interface Props {
  workId: number;
}

/** Records a read of one work. Needs the server: there is no offline
 * queue, so the controls are disabled while offline. */
export function ReadControls({ workId }: Props) {
  const online = useSyncState().online;
  const today = localToday();
  const [date, setDate] = useState(today);
  const [chapterUpdate, setChapterUpdate] = useState(false);
  const [pending, setPending] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const mark = async () => {
    const event = { work_id: workId, date: date || today, chapter_update: chapterUpdate };
    setPending(true);
    setError(null);
    try {
      await reads.record(event);
      showReadToast(event);
      setChapterUpdate(false);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setPending(false);
    }
  };

  return (
    <>
      <div className="read-actions">
        <button type="button" className="button-secondary" disabled={!online || pending} onClick={() => void mark()}>
          {online ? "Mark as read" : "Mark as read (needs the library server)"}
        </button>
        <input
          type="date"
          aria-label="Date read"
          value={date}
          max={today}
          disabled={!online || pending}
          onChange={(e) => setDate(e.target.value)}
        />
        <label>
          <input
            type="checkbox"
            checked={chapterUpdate}
            disabled={!online || pending}
            onChange={(e) => setChapterUpdate(e.target.checked)}
          />
          New chapter only
        </label>
      </div>
      {error && <p className="hint">Couldn’t mark as read: {error}</p>}
    </>
  );
}
