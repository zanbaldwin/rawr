import { routeStore } from "../router";
import { useStore } from "../hooks/useStore";
import { SyncChip } from "./SyncChip";

const LINKS = [
  { hash: "#/", label: "Library", route: "list" },
  { hash: "#/upload", label: "Upload", route: "upload" },
  { hash: "#/stats", label: "Stats", route: "stats" },
  { hash: "#/about", label: "About", route: "about" },
] as const;

export function Header() {
  const { route } = useStore(routeStore);
  return (
    <header className="app-header">
      <a className="brand" href="#/">
        🦖 rawr
      </a>
      <nav>
        {LINKS.map((link) => (
          <a key={link.route} href={link.hash} className={route.name === link.route ? "is-active" : ""}>
            {link.label}
          </a>
        ))}
      </nav>
      <SyncChip />
    </header>
  );
}
