import { navigate, routeToHash } from "../router";
import { queryToParams } from "../hooks/useListQuery";
import type { Library, Query, SortKey } from "../types";

interface Props {
  library: Library;
  query: Query;
  /** Live (undeferred) text so the input never lags typing. */
  text: string;
  counts: Uint32Array;
}

/** Plain controls writing filter state into the URL hash. Edits use
 * replace so typing doesn't fill the history stack. */
export function FilterBar({ library, query, text, counts }: Props) {
  const apply = (next: Query, nextText?: string) => {
    const params = queryToParams(library, { ...next, q: (nextText ?? text).trim().toLowerCase() });
    if (nextText !== undefined && nextText.trim() !== "") params.set("q", nextText);
    navigate(routeToHash({ name: "list" }, params), { replace: true });
  };

  // Fandoms with non-zero counts under the other filters, alphabetical.
  const fandomOptions: { index: number; label: string }[] = [];
  for (let i = 0; i < library.dict.fandoms.length; i++) {
    const count = counts[i]!;
    if (count > 0 || i === query.fandom) {
      fandomOptions.push({ index: i, label: `${library.dict.fandoms[i]!} (${count})` });
    }
  }
  fandomOptions.sort((a, b) => a.label.localeCompare(b.label));

  return (
    <div className="filter-bar">
      <input
        type="search"
        placeholder="Title or author…"
        value={text}
        onChange={(e) => apply(query, e.target.value)}
        aria-label="Search"
      />
      <select
        value={query.fandom}
        onChange={(e) => apply({ ...query, fandom: Number(e.target.value) })}
        aria-label="Fandom"
      >
        <option value={-1}>All fandoms</option>
        {fandomOptions.map((option) => (
          <option key={option.index} value={option.index}>
            {option.label}
          </option>
        ))}
      </select>
      <select
        value={query.rating}
        onChange={(e) => apply({ ...query, rating: Number(e.target.value) })}
        aria-label="Rating"
      >
        <option value={-1}>Any rating</option>
        {library.enums.ratings.map((rating) => (
          <option key={rating.code} value={rating.code}>
            {rating.label}
          </option>
        ))}
      </select>
      <select
        value={query.complete}
        onChange={(e) => apply({ ...query, complete: Number(e.target.value) })}
        aria-label="Completion"
      >
        <option value={-1}>Any status</option>
        <option value={1}>Complete</option>
        <option value={0}>In progress</option>
      </select>
      <select
        value={query.sort}
        onChange={(e) => apply({ ...query, sort: e.target.value as SortKey })}
        aria-label="Sort"
      >
        <option value="recent">Recently added</option>
        <option value="updated">Recently updated</option>
        <option value="words">Longest</option>
        <option value="title">Title</option>
      </select>
    </div>
  );
}
