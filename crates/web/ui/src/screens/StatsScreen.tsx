import { useEffect, useMemo, useState } from "react";
import { computeStats } from "../data/stats";
import type { NamedCount } from "../data/stats";
import { formatBytes, formatNumber, formatWords } from "../format";
import { useSyncState } from "../hooks/useSync";
import type { Library } from "../types";
import type { StorageStatsDto } from "../types/generated/StorageStatsDto";

interface Props {
  library: Library;
}

export function StatsScreen({ library }: Props) {
  const stats = useMemo(() => computeStats(library), [library]);
  const online = useSyncState().online;
  const [sizes, setSizes] = useState<StorageStatsDto | null>(null);

  useEffect(() => {
    if (!online) return;
    let cancelled = false;
    fetch("/api/v1/stats")
      .then((res) => (res.ok ? (res.json() as Promise<StorageStatsDto>) : null))
      .then((data) => {
        if (!cancelled && data) setSizes(data);
      })
      .catch(() => undefined);
    return () => {
      cancelled = true;
    };
  }, [online]);

  const ratio = sizes && sizes.file_size > 0 ? (sizes.content_size / sizes.file_size).toFixed(1) : null;

  return (
    <section className="stats">
      <h1>Stats</h1>

      <div className="stat-tiles">
        <Tile label="Works" value={formatNumber(stats.works)} />
        <Tile label="Versions" value={formatNumber(stats.versions)} />
        <Tile label="Words" value={formatWords(stats.totalWords)} />
        <Tile label="Complete" value={`${formatNumber(stats.complete)} / ${formatNumber(stats.works)}`} />
        <Tile label="On disk" value={sizes ? formatBytes(sizes.file_size) : "—"} />
        <Tile
          label="Uncompressed"
          value={sizes ? `${formatBytes(sizes.content_size)}${ratio ? ` (${ratio}×)` : ""}` : "—"}
        />
      </div>
      {!sizes && <p className="hint">Byte totals need the library server.</p>}

      <CountTable title="Ratings" rows={stats.ratings} />
      <CountTable title="Languages" rows={stats.languages} />
      <CountTable title="Top fandoms" rows={stats.topFandoms} />
      <CountTable title="Top relationships" rows={stats.topRelationships} />
      <CountTable title="Top characters" rows={stats.topCharacters} />
      <CountTable title="Top tags" rows={stats.topFreeforms} />

      <dl className="meta-table">
        <dt>Tags</dt>
        <dd>
          {formatNumber(stats.tagRefs)} across {formatNumber(stats.uniqueTags)} unique
        </dd>
        <dt>Series</dt>
        <dd>
          {formatNumber(stats.seriesCount)} ({formatNumber(stats.worksInSeries)} works in one)
        </dd>
      </dl>
    </section>
  );
}

function Tile({ label, value }: { label: string; value: string }) {
  return (
    <div className="stat-tile">
      <span className="stat-value">{value}</span>
      <span className="stat-label">{label}</span>
    </div>
  );
}

function CountTable({ title, rows }: { title: string; rows: NamedCount[] }) {
  if (rows.length === 0) return null;
  const max = rows[0]?.count ?? 1;
  return (
    <section>
      <h2>{title}</h2>
      <table className="count-table">
        <tbody>
          {rows.map((row) => (
            <tr key={row.name}>
              <td className="count-name">{row.name}</td>
              <td className="count-value">{formatNumber(row.count)}</td>
              <td className="count-bar">
                <span style={{ width: `${Math.max(2, (row.count / max) * 100)}%` }} />
              </td>
            </tr>
          ))}
        </tbody>
      </table>
    </section>
  );
}
