import { useRef, useState } from "react";
import { uploadAll, uploadFile } from "../api/upload";
import { sync } from "../data/instance";
import { useSyncState } from "../hooks/useSync";
import type { UploadOutcome } from "../types/generated/UploadOutcome";

type JobState =
  | { phase: "queued" }
  | { phase: "uploading"; fraction: number }
  | { phase: "done"; outcome: UploadOutcome }
  | { phase: "failed"; message: string };

interface Job {
  name: string;
  state: JobState;
}

const OUTCOME_LABEL: Record<UploadOutcome["status"], string> = {
  added: "Added",
  upgraded: "Upgraded",
  unchanged: "Already in the library",
  outdated: "Older than the library copy",
  failed: "Failed",
};

export function UploadScreen() {
  const state = useSyncState();
  const [jobs, setJobs] = useState<Job[]>([]);
  const [busy, setBusy] = useState(false);
  const inputRef = useRef<HTMLInputElement | null>(null);

  const start = async (files: File[]) => {
    if (files.length === 0) return;
    setBusy(true);
    setJobs(files.map((file) => ({ name: file.name, state: { phase: "queued" } })));
    const patch = (index: number, jobState: JobState) => {
      setJobs((prev) => prev.map((job, i) => (i === index ? { ...job, state: jobState } : job)));
    };
    let changed = false;
    // One request per file, two at a time.
    await uploadAll(files, 2, async (file, index) => {
      patch(index, { phase: "uploading", fraction: 0 });
      try {
        const response = await uploadFile(file, (fraction) => patch(index, { phase: "uploading", fraction }));
        const outcome = response.results[0]?.outcome ?? { status: "failed", message: "empty response", kind: "api" };
        if (outcome.status === "added" || outcome.status === "upgraded" || outcome.status === "outdated") {
          changed = true;
        }
        patch(index, { phase: "done", outcome });
      } catch (error) {
        patch(index, { phase: "failed", message: String(error) });
      }
    });
    setBusy(false);
    if (changed) void sync.refresh();
  };

  if (!state.online) {
    return (
      <section className="upload">
        <h1>Upload</h1>
        <p className="empty-state">
          Uploading needs the library server, and it isn’t reachable right now. (There’s no offline upload queue on
          purpose — Safari has no way to finish it later.)
        </p>
      </section>
    );
  }

  return (
    <section className="upload">
      <h1>Upload</h1>
      <p className="hint">
        AO3 HTML downloads (compressed is fine). Files import straight into the library; duplicates and older versions
        are recognised and skipped.
      </p>
      <input
        ref={inputRef}
        type="file"
        multiple
        // Deliberately loose: the iOS Files picker hides legitimate files
        // under a stricter accept, and the server treats the extension as
        // a compression hint only.
        onChange={(e) => {
          void start(Array.from(e.target.files ?? []));
          e.target.value = "";
        }}
        disabled={busy}
      />
      {jobs.length > 0 && (
        <ul className="upload-jobs">
          {jobs.map((job, index) => (
            <li key={`${job.name}-${index}`}>
              <span className="upload-name">{job.name}</span>
              <span className="upload-state">{describe(job.state)}</span>
              {job.state.phase === "uploading" && (
                <progress value={job.state.fraction} max={1} aria-label={`Uploading ${job.name}`} />
              )}
            </li>
          ))}
        </ul>
      )}
    </section>
  );
}

function describe(state: JobState): string {
  switch (state.phase) {
    case "queued":
      return "Waiting…";
    case "uploading":
      return `Uploading ${Math.round(state.fraction * 100)}%`;
    case "done": {
      const outcome = state.outcome;
      if (outcome.status === "failed") return `Failed: ${outcome.message}`;
      return `${OUTCOME_LABEL[outcome.status]} — ${outcome.work.title}`;
    }
    case "failed":
      return `Failed: ${state.message}`;
  }
}
