import { FilterBar } from "../components/FilterBar";
import { VirtualList } from "../components/VirtualList";
import { formatNumber } from "../format";
import { useListQuery } from "../hooks/useListQuery";
import { useScrollRestore } from "../hooks/useScrollRestore";
import type { Library } from "../types";

interface Props {
  library: Library;
  params: URLSearchParams;
}

export function ListScreen({ library, params }: Props) {
  const { query, rows, counts, stale } = useListQuery(library, params);
  useScrollRestore(`list:${params.toString()}`, true);
  return (
    <>
      <div className="list-header">
        <FilterBar library={library} query={query} text={params.get("q") ?? ""} counts={counts} />
        <p className="result-count">
          {formatNumber(rows.length)} of {formatNumber(library.count)} works
        </p>
      </div>
      {rows.length === 0 ? (
        <p className="empty-state">Nothing matches those filters.</p>
      ) : (
        <VirtualList library={library} rows={rows} stale={stale} />
      )}
    </>
  );
}
