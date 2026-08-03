import { useEffect, useState } from "react";
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
      <FandomCombo library={library} query={query} options={fandomOptions} apply={apply} />
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

/** Searchable fandom filter via native <datalist> — the zero-dependency
 * stopgap while the hand-rolled combobox is pending. Free text commits
 * only on an exact (case-insensitive) fandom match or when cleared;
 * anything else leaves the active filter untouched, and blur snaps the
 * box back so it never lies about what's applied. */
function FandomCombo({
  library,
  query,
  options,
  apply,
}: {
  library: Library;
  query: Query;
  options: { index: number; label: string }[];
  apply: (next: Query) => void;
}) {
  const activeName = query.fandom !== -1 ? (library.dict.fandoms[query.fandom] ?? "") : "";
  const [text, setText] = useState(activeName);

  // External changes (back/forward, URL edits) win over stale typing.
  useEffect(() => {
    setText(activeName);
  }, [activeName]);

  const resolve = (value: string): number => {
    const needle = value.trim().toLowerCase();
    if (needle === "") return -1;
    return library.dict.fandoms.findIndex((name) => name.toLowerCase() === needle);
  };

  const onChange = (value: string) => {
    setText(value);
    if (value.trim() === "") {
      if (query.fandom !== -1) apply({ ...query, fandom: -1 });
      return;
    }
    const index = resolve(value);
    if (index !== -1 && index !== query.fandom) apply({ ...query, fandom: index });
  };

  return (
    <>
      <input
        type="text"
        list="fandom-options"
        placeholder="All fandoms"
        value={text}
        onChange={(e) => onChange(e.target.value)}
        onBlur={() => {
          if (text.trim() !== "" && resolve(text) === -1) setText(activeName);
        }}
        aria-label="Fandom"
        autoCorrect="off"
        autoCapitalize="off"
      />
      <datalist id="fandom-options">
        {options.map((option) => (
          <option key={option.index} value={library.dict.fandoms[option.index]!} label={option.label} />
        ))}
      </datalist>
    </>
  );
}
