// One XHR per file: fetch has no upload-progress events and Safari lacks
// request-body streaming, so this is the app's only justified XHR.

import type { UploadResponse } from "../types/generated/UploadResponse";

export function uploadFile(file: File, onProgress: (fraction: number) => void): Promise<UploadResponse> {
  return new Promise((resolve, reject) => {
    const xhr = new XMLHttpRequest();
    xhr.open("POST", "/api/v1/uploads");
    xhr.responseType = "json";
    xhr.upload.onprogress = (event) => {
      if (event.lengthComputable && event.total > 0) onProgress(event.loaded / event.total);
    };
    xhr.onload = () => {
      // Every API response is JSON, always — including errors.
      const body = xhr.response as UploadResponse | { message?: string } | null;
      if (xhr.status === 200 && body && "results" in body) {
        resolve(body);
      } else {
        const message = body && "message" in body && body.message ? body.message : `server answered ${xhr.status}`;
        reject(new Error(message));
      }
    };
    xhr.onerror = () => reject(new Error("network error"));
    xhr.onabort = () => reject(new Error("cancelled"));
    const form = new FormData();
    form.append("file", file, file.name);
    xhr.send(form);
  });
}

/** Run jobs with bounded concurrency, resolving when all settle. */
export async function uploadAll(
  files: readonly File[],
  concurrency: number,
  run: (file: File, index: number) => Promise<void>,
): Promise<void> {
  let next = 0;
  const workers = Array.from({ length: Math.min(concurrency, files.length) }, async () => {
    for (;;) {
      const index = next++;
      const file = files[index];
      if (file === undefined) return;
      await run(file, index);
    }
  });
  await Promise.all(workers);
}
