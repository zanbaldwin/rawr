import { ErrorBoundary } from "./components/ErrorBoundary";
import { Header } from "./components/Header";
import { OfflineBanner } from "./components/OfflineBanner";
import { sync } from "./data/instance";
import { formatBytes } from "./format";
import { useStore } from "./hooks/useStore";
import { useLibrary, useSyncState } from "./hooks/useSync";
import { routeStore } from "./router";
import { AboutScreen } from "./screens/AboutScreen";
import { ListScreen } from "./screens/ListScreen";
import { WorkScreen } from "./screens/WorkScreen";

export function App() {
  return (
    <ErrorBoundary label="the app">
      <Header />
      <OfflineBanner />
      <main>
        <ErrorBoundary label="this screen">
          <Screen />
        </ErrorBoundary>
      </main>
    </ErrorBoundary>
  );
}

function Screen() {
  const { route, params } = useStore(routeStore);
  const library = useLibrary();
  const state = useSyncState();

  // The one screen the app cannot render: no cache and no server.
  if (state.status === "failed" && library === null) {
    return (
      <div className="full-state">
        <h1>Nothing here yet</h1>
        <p>Nothing’s cached on this device, and the library server isn’t answering. Is the NAS awake?</p>
        <p className="hint">{state.error}</p>
        <button type="button" className="button-primary" onClick={() => void sync.refresh()}>
          Try again
        </button>
      </div>
    );
  }

  if (library === null) {
    const progress =
      state.total !== null && state.total > 0
        ? `${formatBytes(state.received)} of ${formatBytes(state.total)}`
        : state.received > 0
          ? `${formatBytes(state.received)} received`
          : "";
    return (
      <div className="full-state">
        <h1>Downloading your library…</h1>
        <p>First run: fetching the whole index. Every later launch starts instantly from this device.</p>
        {state.total !== null && state.total > 0 && (
          <progress value={state.received} max={state.total} aria-label="Download progress" />
        )}
        <p className="hint">{progress}</p>
      </div>
    );
  }

  switch (route.name) {
    case "list":
      return <ListScreen library={library} params={params} />;
    case "work":
      return <WorkScreen library={library} id={route.id} />;
    case "upload":
      return <p className="empty-state">Uploads are on the way — this screen lands with the upload endpoint.</p>;
    case "stats":
      return <p className="empty-state">Stats are on the way.</p>;
    case "about":
      return <AboutScreen />;
    case "notfound":
      return <p className="empty-state">That page doesn’t exist.</p>;
  }
}
