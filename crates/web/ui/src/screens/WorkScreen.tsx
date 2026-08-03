import { useEffect, useMemo, useState } from "react";
import { formatBytes, formatChapters, formatDay, formatWords } from "../format";
import { useSyncState } from "../hooks/useSync";
import type { Library } from "../types";
import type { WorkDetail } from "../types/generated/WorkDetail";

interface Props {
  library: Library;
  id: number;
}

/** Renders immediately and completely from the index — this IS the
 * offline experience, never blank. The detail endpoint adds the
 * versions/files table when the server answers. */
export function WorkScreen({ library, id }: Props) {
  const row = useMemo(() => {
    for (let i = 0; i < library.count; i++) {
      if (library.id[i] === id) return i;
    }
    return -1;
  }, [library, id]);

  const [detail, setDetail] = useState<WorkDetail | null>(null);
  const [detailError, setDetailError] = useState<string | null>(null);
  const online = useSyncState().online;

  useEffect(() => {
    let cancelled = false;
    setDetail(null);
    setDetailError(null);
    fetch(`/api/v1/works/${id}`)
      .then((res) => {
        if (!res.ok) throw new Error(`server answered ${res.status}`);
        return res.json() as Promise<WorkDetail>;
      })
      .then((data) => {
        if (!cancelled) setDetail(data);
      })
      .catch((error: unknown) => {
        if (!cancelled) setDetailError(String(error));
      });
    return () => {
      cancelled = true;
    };
  }, [id]);

  if (row === -1) {
    return <p className="empty-state">Work {id} isn’t in the synced library. Try refreshing.</p>;
  }

  const authors = spans(library.authors, row).map((a) => {
    const author = library.dict.authors[a]!;
    return author.p ? `${author.p} (${author.u})` : author.u;
  });
  const fandoms = spans(library.fandoms, row).map((f) => library.dict.fandoms[f]!);
  const tags = spans(library.tags, row);
  const rating = library.enums.ratings.find((r) => r.code === library.rating[row]);
  const warnings = library.enums.warnings.filter((w) => (library.warnings[row]! & w.code) !== 0);
  const summary = library.summary[row];
  const downloadUrl = `/api/v1/versions/${library.cid[row]}/download.epub`;

  return (
    <article className="work-detail">
      <header>
        <h1>{library.title[row]}</h1>
        <p className="work-byline">
          by {authors.length > 0 ? authors.join(", ") : "Anonymous"} · {fandoms.join(" · ")}
        </p>
      </header>

      <div className="detail-actions">
        {online ? (
          <a className="button-primary" href={downloadUrl} target="_blank" rel="noopener">
            Download EPUB
          </a>
        ) : (
          <button type="button" className="button-primary" disabled>
            Download EPUB (needs the library server)
          </button>
        )}
        <a className="button-secondary" href={`https://archiveofourown.org/works/${id}`} target="_blank" rel="noopener">
          Open on AO3
        </a>
      </div>
      {online && <p className="hint">Downloads open outside the app; send the file to Books from the Share sheet.</p>}

      <dl className="meta-table">
        <dt>Rating</dt>
        <dd>{rating?.label ?? "Unknown"}</dd>
        <dt>Warnings</dt>
        <dd>{warnings.length > 0 ? warnings.map((w) => w.label).join(", ") : "None"}</dd>
        <dt>Words</dt>
        <dd>{formatWords(library.words[row]!)}</dd>
        <dt>Chapters</dt>
        <dd>
          {formatChapters(library.chapters[row]!, library.chaptersTotal[row]!)}
          {library.complete[row] === 1 ? " (complete)" : ""}
        </dd>
        <dt>Published</dt>
        <dd>{formatDay(library.published[row]!)}</dd>
        <dt>Updated</dt>
        <dd>{formatDay(library.updated[row]!)}</dd>
        <dt>Language</dt>
        <dd>{library.dict.languages[library.language[row]!]?.name ?? "?"}</dd>
      </dl>

      {summary !== null && summary !== undefined && (
        <section className="work-summary">
          <h2>Summary</h2>
          {/* Raw Markdown displayed as text — rendering unescaped HTML from
              fic metadata is not on the table. */}
          <p className="prewrap">{summary}</p>
        </section>
      )}

      {tags.length > 0 && (
        <section>
          <h2>Tags</h2>
          <ul className="tag-list">
            {tags.map((tag) => (
              <li key={tag} className={`tag-kind-${library.dict.tagKinds[tag]}`}>
                {library.dict.tags[tag]}
              </li>
            ))}
          </ul>
        </section>
      )}

      <section>
        <h2>Versions</h2>
        {detail ? (
          <table className="versions-table">
            <thead>
              <tr>
                <th>id</th>
                <th>words</th>
                <th>size</th>
                <th>file</th>
                <th></th>
              </tr>
            </thead>
            <tbody>
              {detail.versions.map((version) => (
                <tr key={version.content_hash} className={version.is_best ? "is-best" : ""}>
                  <td>
                    <code>{version.id}</code>
                    {version.is_best ? " ★" : ""}
                  </td>
                  <td>{formatWords(version.metadata.words)}</td>
                  <td>{formatBytes(version.length)}</td>
                  <td className="path-cell">
                    {version.files.map((file) => (
                      <code key={file.path}>{file.path}</code>
                    ))}
                  </td>
                  <td>
                    {online && (
                      <a href={version.epub_url} target="_blank" rel="noopener">
                        EPUB
                      </a>
                    )}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        ) : detailError ? (
          <p className="hint">Version details need the library server ({detailError}).</p>
        ) : (
          <p className="hint">Loading version details…</p>
        )}
      </section>
    </article>
  );
}

function spans(csr: { offsets: Uint32Array; values: Uint32Array }, row: number): number[] {
  const out: number[] = [];
  for (let cursor = csr.offsets[row]!; cursor < csr.offsets[row + 1]!; cursor++) {
    out.push(csr.values[cursor]!);
  }
  return out;
}
