import { SitesOverview } from "./components/SitesOverview";
import { RuntimePanel } from "./components/RuntimePanel";
import { Profiles } from "./components/Profiles";
import { ErrorNotice } from "./components/ErrorNotice";
import { NewProject } from "./components/NewProject";
import { DatabaseManager } from "./components/DatabaseManager";
import { DeveloperSettings, Diagnostics } from "./components/DeveloperSettings";
import { ProjectLocations } from "./components/ProjectLocations";
import { listen } from "@tauri-apps/api/event";
import { ToolManager } from "./components/ToolManager";
import { SiteDetail } from "./components/SiteDetail";
import { ImportDialog, PhpDialog } from "./components/RuntimeDialogs";
import { ProductSettings } from "./components/ProductSettings";
import type { InstallProgress } from "./contracts";
import { Logs } from "./components/Logs";
import { useCallback, useEffect, useState } from "react";
import { bridge, desktop } from "./bridge";
import type { Action, Snapshot, RuntimeKind, Installation } from "./contracts";
import { availableSiteCount } from "./presentation";
type Page =
  | "New Project"
  | "Sites"
  | "Runtimes"
  | "Profiles"
  | "Tools"
  | "Databases"
  | "Logs"
  | "Settings";
const pages: Page[] = [
  "Sites",
  "New Project",
  "Runtimes",
  "Profiles",
  "Databases",
  "Tools",
  "Logs",
  "Settings",
];
export default function App() {
  const [logSite, setLogSite] = useState<
    Snapshot["sites"][number] | undefined
  >();
  const [installing, setInstalling] = useState(false);
  const [progress, setProgress] = useState<InstallProgress | null>(null);
  const [catalogUrl, setCatalogUrl] = useState("");
  const [catalogHash, setCatalogHash] = useState("");
  const [page, setPage] = useState<Page>("Sites");
  const [data, setData] = useState<Snapshot | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const [pollError, setPollError] = useState("");
  const [pending, setPending] = useState<Action | null>(null);
  const [notice, setNotice] = useState("");
  const [selected, setSelected] = useState<string | null>(null);
  const [importKind, setImportKind] = useState<RuntimeKind | null>(null);
  const [php, setPhp] = useState<Installation | null>(null);
  useEffect(() => {
    if (!desktop) return;
    const subscription = listen("devone-new-project", () => {
      setPage("New Project");
      setSelected(null);
    });
    return () => {
      void subscription.then((unlisten) => unlisten());
    };
  }, []);
  const refresh = useCallback(async () => {
    if (desktop) {
      try {
        setData(await bridge.snapshot());
        setPollError("");
      } catch (e) {
        setPollError(String(e));
      }
    }
  }, []);
  useEffect(() => {
    void refresh();
  }, [refresh]);
  useEffect(() => {
    if (!desktop || busy) return;
    const id = setInterval(() => void refresh(), 4000);
    return () => clearInterval(id);
  }, [refresh, busy]);
  useEffect(() => {
    if (!installing || !desktop) return;
    const id = setInterval(() => {
      void bridge
        .progress()
        .then(setProgress)
        .catch(() => {});
    }, 500);
    return () => clearInterval(id);
  }, [installing]);
  const act = async (action: Action) => {
    setProgress(null);
    setInstalling(action.type === "install");
    setBusy(true);
    setPending(action);
    setError("");
    setNotice("");
    try {
      const result = await bridge.execute(action);
      setData(result.snapshot);
      if (result.message) setNotice(result.message);
      return true;
    } catch (e) {
      setError(String(e));
      return false;
    } finally {
      setBusy(false);
      setPending(null);
      setInstalling(false);
      void refresh();
    }
  };
  const site = data?.sites.find((s) => s.id === selected);
  const count = data ? availableSiteCount(data.sites) : 0;
  return (
    <div className="app">
      <aside className="sidebar">
        <div className="brand">
          <span className="brand-mark">D</span>
          <div>
            DEVONE<span>LOCAL DEVELOPMENT</span>
          </div>
        </div>
        <div className="workspace-label">WORKSPACE</div>
        <nav>
          {pages.map((p, i) => (
            <button
              key={p}
              className={page === p ? "nav active" : "nav"}
              onClick={() => {
                setPage(p);
                setLogSite(undefined);
                setSelected(null);
              }}
            >
              <span className="nav-icon" aria-hidden="true">
                {
                  [
                    "\u25eb",
                    "+",
                    "\u25c7",
                    "\u25ce",
                    "\u25a4",
                    "\u2692",
                    "\u2261",
                    "\u2699",
                  ][i]
                }
              </span>
              {p}
              {p === "Sites" && <span className="count">{count}</span>}
            </button>
          ))}
        </nav>
        <div className="sidebar-bottom">
          <span className={"dot " + (data?.active ? "online" : "")} />
          {data?.active ? "Environment active" : "Environment stopped"}
          <small>Native runtimes · isolated versions</small>
        </div>
      </aside>
      <main>
        <header className="topbar">
          <span>
            Local workspace <span className="slash">/</span> {page}
          </span>
          <span className="platform">
            {data?.platform ?? "Desktop connection required"}
          </span>
        </header>
        <div className="content">
          <div className="heading">
            <div>
              <p className="eyebrow">YOUR DEVELOPMENT ENVIRONMENT</p>
              <h1>{site ? site.hostname : page}</h1>
              <p>
                {page === "Sites"
                  ? "Projects from your selected www folder and registered folders."
                  : page === "Runtimes"
                    ? "Install once. Run different versions side by side."
                    : page === "Databases"
                      ? "Your local MySQL databases and backups."
                      : page === "Logs"
                        ? "Choose a log stream to investigate a problem."
                        : "One home for your projects and native tools."}
              </p>
            </div>
            {page === "Sites" && (
              <div className="actions">
                {page === "Sites" && (
                  <button
                    disabled={busy || !desktop}
                    onClick={() => {
                      setPage("New Project");
                      setSelected(null);
                    }}
                  >
                    New Project
                  </button>
                )}
                <button
                  disabled={busy || !desktop}
                  onClick={() => void act({ type: "stop" })}
                >
                  Stop All
                </button>
                <button
                  disabled={busy || !desktop}
                  onClick={() => void act({ type: "restart" })}
                >
                  Restart
                </button>
                <button
                  className="primary"
                  disabled={busy || !desktop}
                  onClick={() => void act({ type: "start" })}
                >
                  {busy ? "Working…" : "▶ Start All"}
                </button>
              </div>
            )}
          </div>
          {!desktop && (
            <div className="alert">
              This browser view is the frontend shell. Launch{" "}
              <code>pnpm desktop</code> to connect to the Rust core. Runtime and
              site data are not simulated.
            </div>
          )}
          {busy && progress && (
            <div className="alert" role="status">
              {progress.runtime} · {progress.phase} ·{" "}
              {progress.bytes.toLocaleString()} bytes{" "}
              {progress.total ? "/ " + progress.total.toLocaleString() : ""}
            </div>
          )}
          {busy && !progress && (
            <p className="alert" role="status">
              {pending?.type === "site_action"
                ? `${pending.operation === "start" ? "Starting" : pending.operation === "stop" ? "Stopping" : "Restarting"} site...`
                : "Working..."}
            </p>
          )}
          <ErrorNotice error={error || pollError} />
          {data && !data.setup.dns_system && (
            <div className="alert">
              Local .test domains are not enabled.{" "}
              <button disabled={busy} onClick={() => void act({ type: "dns" })}>
                Enable
              </button>
            </div>
          )}
          {data && !data.setup.ca_trusted && (
            <div className="alert">
              HTTPS CA is not trusted.{" "}
              <button disabled={busy} onClick={() => void act({ type: "trust" })}>
                Trust CA
              </button>
            </div>
          )}
          {notice && (
            <div className="alert success" style={{ whiteSpace: "pre-wrap" }}>
              {notice}
            </div>
          )}
          {data?.issues.map((issue, i) => (
            <div className="alert" key={i}>
              {issue}
            </div>
          ))}
          {!data && desktop ? (
            <div className="empty">Connecting to the local core…</div>
          ) : (
            data && (
              <>
                {page === "Profiles" && (
                  <Profiles data={data} busy={busy} act={act} />
                )}
                {page === "Sites" &&
                  (site ? (
                    <SiteDetail
                      site={site}
                      data={data}
                      busy={busy}
                      act={act}
                      logs={() => {
                        setLogSite(site);
                        setPage("Logs");
                      }}
                      back={() => setSelected(null)}
                    />
                  ) : (
                    <SitesOverview
                      data={data}
                      busy={busy}
                      act={act}
                      view={setSelected}
                      create={() => setPage("New Project")}
                    />
                  ))}
                {page === "Tools" && (
                  <ToolManager data={data} busy={busy} act={act} />
                )}
                {page === "Runtimes" && (
                  <RuntimePanel
                    data={data}
                    busy={busy}
                    act={act}
                    importRuntime={setImportKind}
                    configure={setPhp}
                  />
                )}
                {page === "Databases" && (
                  <DatabaseManager
                    data={data}
                    busy={busy}
                    act={act}
                    logs={() => {
                      setLogSite(undefined);
                      setPage("Logs");
                    }}
                  />
                )}
                {page === "New Project" && (
                  <NewProject
                    data={data}
                    busy={busy}
                    act={act}
                    view={(id) => {
                      setSelected(id);
                      setPage("Sites");
                    }}
                  />
                )}
                {page === "Logs" && (
                  <Logs key={logSite?.id ?? "all"} site={logSite} />
                )}
                {page === "Settings" && (
                  <div className="settings-page">
                    <button
                      className="reopen-setup"
                      disabled={busy}
                      onClick={() => {
                        setPage("Settings");
                        void act({ type: "reopen_setup" });
                      }}
                    >
                      Review configuration
                    </button>
                    <section className="panel">
                      <div className="panel-header">
                        <h2>General</h2>
                      </div>
                      <dl>
                        <dt>Home</dt>
                        <dd>
                          <code>{data.home}</code>
                        </dd>
                        <dt>Platform</dt>
                        <dd>{data.platform}</dd>
                        <dt>Projects</dt>
                        <dd>
                          <code>
                            {data.projects_root ?? `${data.home}/www`}
                          </code>
                        </dd>
                        <dt>Home override</dt>
                        <dd>
                          Set DEVONE_HOME before launching. Restart required.
                        </dd>
                      </dl>
                      <ProjectLocations data={data} busy={busy} act={act} />
                    </section>
                    <ProductSettings data={data} busy={busy} act={act} />
                    <DeveloperSettings data={data} busy={busy} act={act} />
                    <details className="panel panel-copy">
                      <summary>Advanced diagnostics</summary>
                      <Diagnostics data={data} busy={busy} act={act} />
                    </details>
                    <section className="panel">
                      <div className="panel-header">
                        <h2>Runtime catalog</h2>
                        <button
                          disabled={busy}
                          onClick={() => void act({ type: "refresh_catalog" })}
                        >
                          Reload catalog
                        </button>
                      </div>
                      <p className="panel-copy">
                        Bundled choices are ready to install. Catalog refresh
                        preserves the last working version when an update fails.
                      </p>
                      <details className="panel-copy">
                        <summary>Advanced catalog sources</summary>
                        <div className="creation-fields">
                          <p>
                            Optional local override: config/runtime-catalog.json
                            inside DEVONE Home.
                          </p>
                          <label>
                            Remote catalog HTTPS URL
                            <input
                              value={catalogUrl}
                              onChange={(e) => setCatalogUrl(e.target.value)}
                            />
                          </label>
                          <label>
                            Expected metadata SHA-256
                            <input
                              value={catalogHash}
                              onChange={(e) => setCatalogHash(e.target.value)}
                            />
                          </label>
                          <button
                            disabled={busy || !catalogUrl || !catalogHash}
                            onClick={() =>
                              void act({
                                type: "remote_catalog",
                                url: catalogUrl,
                                sha256: catalogHash,
                              })
                            }
                          >
                            Verify & refresh remote catalog
                          </button>
                        </div>
                      </details>
                    </section>
                  </div>
                )}
              </>
            )
          )}
        </div>
        <footer>
          DEVONE LOCAL <span>PHP · Node · Static</span>
        </footer>
      </main>
      {importKind && data && (
        <ImportDialog
          kind={importKind}
          data={data}
          busy={busy}
          error={error}
          close={() => setImportKind(null)}
          act={act}
        />
      )}
      {php && data && (
        <PhpDialog
          runtime={php}
          data={data}
          busy={busy}
          error={error}
          close={() => setPhp(null)}
          act={act}
        />
      )}
    </div>
  );
}
