// Library statistics derived client-side from the index — one pass over
// the columns and the tag CSR (~2 ms at 10k works), instant and
// offline-capable. Only byte totals come from the server.

import type { Library } from "../types";

export interface NamedCount {
  name: string;
  count: number;
}

export interface LibraryStats {
  works: number;
  versions: number;
  totalWords: number;
  complete: number;
  incomplete: number;
  ratings: NamedCount[];
  languages: NamedCount[];
  topFandoms: NamedCount[];
  topRelationships: NamedCount[];
  topCharacters: NamedCount[];
  topFreeforms: NamedCount[];
  tagRefs: number;
  uniqueTags: number;
  seriesCount: number;
  worksInSeries: number;
}

const TOP_N = 10;

function top(names: readonly string[], counts: Uint32Array, keep: (i: number) => boolean): NamedCount[] {
  const entries: NamedCount[] = [];
  for (let i = 0; i < counts.length; i++) {
    if (counts[i]! > 0 && keep(i)) entries.push({ name: names[i]!, count: counts[i]! });
  }
  entries.sort((a, b) => b.count - a.count || a.name.localeCompare(b.name));
  return entries.slice(0, TOP_N);
}

export function computeStats(library: Library): LibraryStats {
  let versions = 0;
  let totalWords = 0;
  let complete = 0;
  let worksInSeries = 0;
  const ratingCounts = new Map<number, number>();
  const languageCounts = new Uint32Array(library.dict.languages.length);
  const fandomCounts = new Uint32Array(library.dict.fandoms.length);
  const tagCounts = new Uint32Array(library.dict.tags.length);
  const seriesSeen = new Set<number>();

  for (let i = 0; i < library.count; i++) {
    versions += library.versions[i]!;
    totalWords += library.words[i]!;
    if (library.complete[i] === 1) complete++;
    const rating = library.rating[i]!;
    ratingCounts.set(rating, (ratingCounts.get(rating) ?? 0) + 1);
    const language = library.language[i]!;
    languageCounts[language] = languageCounts[language]! + 1;
    for (let c = library.fandoms.offsets[i]!; c < library.fandoms.offsets[i + 1]!; c++) {
      const fandom = library.fandoms.values[c]!;
      fandomCounts[fandom] = fandomCounts[fandom]! + 1;
    }
    for (let c = library.tags.offsets[i]!; c < library.tags.offsets[i + 1]!; c++) {
      const tag = library.tags.values[c]!;
      tagCounts[tag] = tagCounts[tag]! + 1;
    }
    const seriesStart = library.series.offsets[i]!;
    const seriesEnd = library.series.offsets[i + 1]!;
    if (seriesEnd > seriesStart) worksInSeries++;
    for (let c = seriesStart; c < seriesEnd; c++) seriesSeen.add(library.series.series[c]!);
  }

  let tagRefs = 0;
  let uniqueTags = 0;
  for (const count of tagCounts) {
    tagRefs += count;
    if (count > 0) uniqueTags++;
  }

  const kindOf = (i: number) => library.dict.tagKinds[i]!;
  return {
    works: library.count,
    versions,
    totalWords,
    complete,
    incomplete: library.count - complete,
    ratings: library.enums.ratings
      .map((r) => ({ name: r.label, count: ratingCounts.get(r.code) ?? 0 }))
      .filter((r) => r.count > 0),
    languages: top(
      library.dict.languages.map((l) => l.name),
      languageCounts,
      () => true,
    ),
    topFandoms: top(library.dict.fandoms, fandomCounts, () => true),
    topRelationships: top(library.dict.tags, tagCounts, (i) => kindOf(i) === 0),
    topCharacters: top(library.dict.tags, tagCounts, (i) => kindOf(i) === 1),
    topFreeforms: top(library.dict.tags, tagCounts, (i) => kindOf(i) === 2),
    tagRefs,
    uniqueTags,
    seriesCount: seriesSeen.size,
    worksInSeries,
  };
}
