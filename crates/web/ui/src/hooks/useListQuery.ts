// URL params ↔ Query conversion (junk-tolerant, both ways) and the
// memoised query execution with deferred search-as-you-type.

import { useDeferredValue, useMemo } from "react";
import { fandomCounts, runQuery } from "../data/query";
import { DEFAULT_QUERY } from "../types";
import type { Library, Query, SortKey } from "../types";
import type { LanguageEntry } from "../types/generated/LanguageEntry";

const SORTS: SortKey[] = ["recent", "title", "words", "updated"];

/** The ISO code, or the name when the work never had one. */
function languageCode(language: LanguageEntry): string {
  return (language.iso ?? language.name).toLowerCase();
}

/** URL → Query. Unknown values fall back to defaults; a hand-typed URL
 * must never 400 the library. The fandom travels by NAME (stable across
 * index refreshes) and resolves to a dictionary index here. */
export function paramsToQuery(library: Library, params: URLSearchParams, qOverride?: string): Query {
  const q = (qOverride ?? params.get("q") ?? "").trim().toLowerCase();
  // Repeated ?fandom= params; names that no longer resolve are dropped.
  const fandoms = params
    .getAll("fandom")
    .map((name) => library.dict.fandoms.indexOf(name))
    .filter((index) => index !== -1);
  const langCode = (params.get("lang") ?? "").toLowerCase();
  const language = library.dict.languages.findIndex((l) => languageCode(l) === langCode);
  const completeRaw = params.get("complete");
  const complete = completeRaw === "1" ? 1 : completeRaw === "0" ? 0 : -1;
  const sortRaw = params.get("sort") as SortKey | null;
  const sort = sortRaw !== null && SORTS.includes(sortRaw) ? sortRaw : DEFAULT_QUERY.sort;
  return { q, fandoms, language, complete, sort };
}

/** Query → URL params, omitting defaults so clean views have clean URLs. */
export function queryToParams(library: Library, query: Query): URLSearchParams {
  const params = new URLSearchParams();
  if (query.q !== "") params.set("q", query.q);
  for (const fandom of query.fandoms) {
    const name = library.dict.fandoms[fandom];
    if (name !== undefined) params.append("fandom", name);
  }
  const language = library.dict.languages[query.language];
  if (language) params.set("lang", languageCode(language));
  if (query.complete !== -1) params.set("complete", String(query.complete));
  if (query.sort !== DEFAULT_QUERY.sort) params.set("sort", query.sort);
  return params;
}

export interface ListQueryResult {
  query: Query;
  rows: Uint32Array;
  counts: Uint32Array;
  /** True while the deferred needle lags the input — dim, don't spinner. */
  stale: boolean;
}

export function useListQuery(library: Library, params: URLSearchParams): ListQueryResult {
  const rawQ = params.get("q") ?? "";
  const deferredQ = useDeferredValue(rawQ);
  const paramsKey = params.toString();
  // Explicit useMemo kept even under the React Compiler: the deferred
  // value must be the only thing that changes between renders while
  // typing.
  const query = useMemo(
    () => paramsToQuery(library, new URLSearchParams(paramsKey), deferredQ),
    [library, paramsKey, deferredQ],
  );
  const rows = useMemo(() => runQuery(library, query), [library, query]);
  const counts = useMemo(() => fandomCounts(library, query), [library, query]);
  return { query, rows, counts, stale: rawQ.trim().toLowerCase() !== deferredQ.trim().toLowerCase() };
}
