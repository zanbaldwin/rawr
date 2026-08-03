// Hand-rolled multi-select fandom combobox (the datalist stopgap's
// replacement): selected fandoms render as removable chips, typing
// filters the dictionary in memory, and options toggle without closing
// the panel. The same component is the template for tag typeahead later.

import { useId, useMemo, useRef, useState } from "react";
import type { Library } from "../types";

interface Props {
  library: Library;
  /** Indices into `dict.fandoms`. */
  selected: readonly number[];
  /** Facet counts (fandom filter excluded), parallel to the dictionary. */
  counts: Uint32Array;
  onChange: (next: number[]) => void;
}

const MAX_RESULTS = 50;

export function FandomSelect({ library, selected, counts, onChange }: Props) {
  const [text, setText] = useState("");
  const [open, setOpen] = useState(false);
  const [highlight, setHighlight] = useState(0);
  const inputRef = useRef<HTMLInputElement | null>(null);
  const listboxId = useId();

  const results = useMemo(() => {
    const needle = text.trim().toLowerCase();
    const matches: { index: number; count: number }[] = [];
    for (let i = 0; i < library.dict.fandoms.length; i++) {
      if (counts[i] === 0 && !selected.includes(i)) continue;
      if (needle !== "" && !library.dict.fandoms[i]!.toLowerCase().includes(needle)) continue;
      matches.push({ index: i, count: counts[i]! });
    }
    // Search results by usefulness; the browse-all list alphabetical.
    if (needle !== "") {
      matches.sort((a, b) => b.count - a.count || library.dict.fandoms[a.index]!.localeCompare(library.dict.fandoms[b.index]!));
    } else {
      matches.sort((a, b) => library.dict.fandoms[a.index]!.localeCompare(library.dict.fandoms[b.index]!));
    }
    return matches.slice(0, MAX_RESULTS);
  }, [library, counts, text, selected]);

  const clampedHighlight = Math.min(highlight, Math.max(0, results.length - 1));

  const toggle = (index: number) => {
    onChange(selected.includes(index) ? selected.filter((s) => s !== index) : [...selected, index]);
    // Stay open for further picks; clear the needle so the next search
    // starts fresh.
    setText("");
    setHighlight(0);
    inputRef.current?.focus();
  };

  const onKeyDown = (event: React.KeyboardEvent) => {
    switch (event.key) {
      case "ArrowDown":
        event.preventDefault();
        setOpen(true);
        setHighlight((h) => Math.min(h + 1, results.length - 1));
        break;
      case "ArrowUp":
        event.preventDefault();
        setHighlight((h) => Math.max(h - 1, 0));
        break;
      case "Enter": {
        const target = results[clampedHighlight];
        if (open && target) {
          event.preventDefault();
          toggle(target.index);
        }
        break;
      }
      case "Escape":
        setOpen(false);
        break;
      case "Backspace":
        if (text === "" && selected.length > 0) {
          onChange(selected.slice(0, -1));
        }
        break;
    }
  };

  return (
    <div className="fandom-select">
      <div className="fandom-control">
        {selected.map((index) => (
          <button
            key={index}
            type="button"
            className="fandom-chip"
            onClick={() => onChange(selected.filter((s) => s !== index))}
            aria-label={`Remove fandom filter ${library.dict.fandoms[index] ?? ""}`}
          >
            {library.dict.fandoms[index]} ✕
          </button>
        ))}
        <input
          ref={inputRef}
          type="text"
          role="combobox"
          aria-expanded={open}
          aria-controls={listboxId}
          aria-activedescendant={open && results[clampedHighlight] ? `${listboxId}-${results[clampedHighlight].index}` : undefined}
          aria-label="Fandoms"
          placeholder={selected.length === 0 ? "All fandoms" : "Add fandom…"}
          value={text}
          onChange={(e) => {
            setText(e.target.value);
            setHighlight(0);
            setOpen(true);
          }}
          onFocus={() => setOpen(true)}
          onBlur={() => setOpen(false)}
          onKeyDown={onKeyDown}
          autoCorrect="off"
          autoCapitalize="off"
        />
      </div>
      {open && results.length > 0 && (
        <ul id={listboxId} role="listbox" aria-multiselectable="true" className="fandom-panel">
          {results.map((result, position) => {
            const name = library.dict.fandoms[result.index]!;
            const isSelected = selected.includes(result.index);
            return (
              <li
                key={result.index}
                id={`${listboxId}-${result.index}`}
                role="option"
                aria-selected={isSelected}
                className={
                  (position === clampedHighlight ? "is-highlighted" : "") + (isSelected ? " is-selected" : "")
                }
                // preventDefault on pointerdown keeps focus in the input,
                // so blur doesn't close the panel before the tap lands.
                onPointerDown={(e) => e.preventDefault()}
                onClick={() => toggle(result.index)}
              >
                <span className="fandom-name">{name}</span>
                <span className="fandom-count">{isSelected ? "✓" : result.count}</span>
              </li>
            );
          })}
        </ul>
      )}
    </div>
  );
}
