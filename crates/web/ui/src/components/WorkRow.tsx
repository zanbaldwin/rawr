import { formatChapters, formatWords } from "../format";
import type { Library } from "../types";

interface Props {
  library: Library;
  row: number;
}

function authorLine(library: Library, row: number): string {
  const { offsets, values } = library.authors;
  const parts: string[] = [];
  for (let cursor = offsets[row]!; cursor < offsets[row + 1]!; cursor++) {
    const author = library.dict.authors[values[cursor]!]!;
    parts.push(author.p ? `${author.p} (${author.u})` : author.u);
  }
  return parts.length > 0 ? parts.join(", ") : "Anonymous";
}

function fandomLine(library: Library, row: number): string {
  const { offsets, values } = library.fandoms;
  const parts: string[] = [];
  for (let cursor = offsets[row]!; cursor < offsets[row + 1]!; cursor++) {
    parts.push(library.dict.fandoms[values[cursor]!]!);
  }
  return parts.join(" · ");
}

export function WorkRow({ library, row }: Props) {
  const rating = library.enums.ratings.find((r) => r.code === library.rating[row])?.short ?? "?";
  const complete = library.complete[row] === 1;
  return (
    <a className="work-row" href={`#/works/${library.id[row]}`}>
      <span className={`rating-badge rating-${rating.toLowerCase()}`}>{rating}</span>
      <span className="work-main">
        <span className="work-title">{library.title[row]}</span>
        <span className="work-byline">{authorLine(library, row)}</span>
        <span className="work-fandoms">{fandomLine(library, row)}</span>
      </span>
      <span className="work-meta">
        <span>{formatWords(library.words[row]!)} words</span>
        <span>
          {formatChapters(library.chapters[row]!, library.chaptersTotal[row]!)}
          {complete ? " ✓" : ""}
        </span>
      </span>
    </a>
  );
}
