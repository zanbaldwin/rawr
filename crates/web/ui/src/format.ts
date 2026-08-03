// Display formatting. Intl only — no date/number dependencies.

const NUMBER = new Intl.NumberFormat();
const DATE = new Intl.DateTimeFormat(undefined, { year: "numeric", month: "short", day: "numeric" });
const RELATIVE = new Intl.RelativeTimeFormat(undefined, { numeric: "auto" });

export function formatNumber(value: number): string {
  return NUMBER.format(value);
}

/** Word counts read best short: 1.2k, 458k, 1.1M. */
export function formatWords(words: number): string {
  if (words >= 1_000_000) return `${(words / 1_000_000).toFixed(1)}M`;
  if (words >= 10_000) return `${Math.round(words / 1_000)}k`;
  if (words >= 1_000) return `${(words / 1_000).toFixed(1)}k`;
  return String(words);
}

export function formatBytes(bytes: number): string {
  if (bytes >= 1 << 30) return `${(bytes / (1 << 30)).toFixed(2)} GiB`;
  if (bytes >= 1 << 20) return `${(bytes / (1 << 20)).toFixed(1)} MiB`;
  if (bytes >= 1 << 10) return `${(bytes / (1 << 10)).toFixed(1)} KiB`;
  return `${bytes} B`;
}

/** `3/10`, `3/?`, `1/1`. */
export function formatChapters(written: number, total: number): string {
  return `${written}/${total < 0 ? "?" : total}`;
}

/** Days-since-epoch → local date string. */
export function formatDay(days: number): string {
  return DATE.format(new Date(days * 86_400_000));
}

/** Unix seconds → "3 hours ago" / "yesterday". */
export function formatAge(unixSeconds: number, nowMs = Date.now()): string {
  const seconds = Math.round(unixSeconds - nowMs / 1000);
  const abs = Math.abs(seconds);
  if (abs < 60) return RELATIVE.format(Math.trunc(seconds), "second");
  if (abs < 3_600) return RELATIVE.format(Math.trunc(seconds / 60), "minute");
  if (abs < 86_400) return RELATIVE.format(Math.trunc(seconds / 3_600), "hour");
  return RELATIVE.format(Math.trunc(seconds / 86_400), "day");
}
