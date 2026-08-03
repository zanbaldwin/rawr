import { useRowHeight, useWindow } from "../hooks/useWindow";
import type { Library } from "../types";
import { WorkRow } from "./WorkRow";

interface Props {
  library: Library;
  rows: Uint32Array;
  stale: boolean;
}

/** One continuous windowed list over the whole filtered set — no
 * pagination, no infinite scroll. Fixed row height makes windowing (and
 * scroll restoration) trivial. */
export function VirtualList({ library, rows, stale }: Props) {
  const rowHeight = useRowHeight();
  const { ref, start, end } = useWindow(rows.length, rowHeight);
  const visible: React.ReactNode[] = [];
  for (let i = start; i < end; i++) {
    const row = rows[i]!;
    visible.push(<WorkRow key={library.id[row]} library={library} row={row} />);
  }
  return (
    <div
      ref={ref}
      className={`virtual-list${stale ? " is-stale" : ""}`}
      style={{ height: rows.length * rowHeight }}
    >
      <div style={{ transform: `translateY(${start * rowHeight}px)` }}>{visible}</div>
    </div>
  );
}
