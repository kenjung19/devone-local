import { Badge } from "./components/Badge";
import { Stat, SiteDetail } from "./components/SiteDetail";
import { ImportDialog, PhpDialog } from "./components/RuntimeDialogs";
import { Setup } from "./components/Setup";
import { ProductSettings } from "./components/ProductSettings";
import type { InstallProgress } from "./contracts";
import { Logs } from "./components/Logs";
import { useCallback, useEffect, useState } from "react";
import { bridge, desktop } from "./bridge";
import type { Action, Snapshot, RuntimeKind, Installation } from "./contracts";
import { bindingLabel, availableSiteCount } from "./presentation";
type Page = "Sites" | "Runtimes" | "Databases" | "Logs" | "Settings";
const pages: Page[] = ["Sites", "Runtimes", "Databases", "Logs", "Settings"];
const kinds: RuntimeKind[] = ["php", "mysql", "caddy"];
export default function App() {
  const [logSite, setLogSite] = useState<
    Snapshot["sites"][number] | undefined
  >();
  const [setupDismissed, setSetupDismissed] = useState(false);
  const [installing, setInstalling] = useState(false);
  const [progress, setProgress] = useState<InstallProgress | null>(null);
  const [catalogUrl, setCatalogUrl] = useState("");
  const [catalogHash, setCatalogHash] = useState("");
  const [page, setPage] = useState<Page>("Sites");
  const [data, setData] = useState<Snapshot | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const [notice, setNotice] = useState("");
  const [selected, setSelected] = useState<string | null>(null);
  const [importKind, setImportKind] = useState<RuntimeKind | null>(null);
  const [php, setPhp] = useState<Installation | null>(null);
  const refresh = useCallback(async () => {
    if (desktop) {
      try {
        setData(await bridge.snapshot());
      } catch (e) {
        setError(String(e));
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
    setError("");
    setNotice("");
    try {
      const result = await bridge.execute(action);
      setData(result.snapshot);
      if (result.message) setNotice(result.message);
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
      setInstalling(false);
      void refresh();
    }
  };
  const site = data?.sites.find((s) => s.id === selected);
  const count = data ? availableSiteCount(data.sites) : 0;
  if (data && !data.setup.completed && !setupDismissed) {
    return (
      <>
        <Setup
          data={data}
          busy={busy}
          error={error}
          progress={progress}
          act={act}
          close={() => {
            setSetupDismissed(true);
            setPage("Settings");
          }}
          importRuntime={setImportKind}
        />
        {importKind && (
          <ImportDialog
            kind={importKind}
            data={data}
            busy={busy}
            error={error}
            close={() => setImportKind(null)}
            act={act}
          />
        )}
      </>
    );
  }

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
              <span className="nav-icon">{["◫", "◇", "▤", "≡", "⚙"][i]}</span>
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
                  ? "Your projects, automatically discovered from www."
                  : page === "Runtimes"
                    ? "Install once. Run different versions side by side."
                    : page === "Databases"
                      ? "Persistent engine instances shared by runtime version."
                      : page === "Logs"
                        ? "Live output from DEVONE-owned services."
                        : "One home for your projects and native tools."}
              </p>
            </div>
            <div className="actions">
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
          {error && (
            <div className="alert error" role="alert">
              {error}
            </div>
          )}
          {notice && <div className="alert success">{notice}</div>}
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
                    <>
                      <div className="stats">
                        <Stat label="DISCOVERED SITES" value={String(count)} />
                        <Stat
                          label="RUNNING SERVICES"
                          value={String(
                            data.services.filter((s) => s.healthy).length,
                          )}
                        />
                        <Stat
                          label="LOCAL DOMAINS"
                          value={data.dns_ready ? "Ready" : "Setup required"}
                        />
                        <Stat
                          label="LOCAL CA"
                          value={data.ca_present ? "Created" : "Not created"}
                        />
                      </div>
                      <div className="panel">
                        <div className="panel-header">
                          <h2>
                            Projects <span className="muted">{count}</span>
                          </h2>
                          <div className="actions">
                            <button
                              disabled={busy}
                              onClick={() =>
                                void act({ type: "open_folder", site_id: null })
                              }
                            >
                              Open www
                            </button>
                            <button
                              disabled={busy}
                              onClick={() => void act({ type: "scan" })}
                            >
                              Rescan
                            </button>
                          </div>
                        </div>
                        {count === 0 ? (
                          <div className="empty">
                            <span className="empty-icon">◫</span>
                            <h3>Your next project starts here</h3>
                            <p>
                              Put a project folder inside{" "}
                              <code>{data.home}/www</code>.<br />
                              DEVONE discovers direct child directories
                              automatically.
                            </p>
                            <button
                              onClick={() =>
                                void act({ type: "open_folder", site_id: null })
                              }
                            >
                              Open project directory
                            </button>
                          </div>
                        ) : (
                          <table>
                            <thead>
                              <tr>
                                <th>PROJECT</th>
                                <th>PHP</th>
                                <th>MYSQL</th>
                                <th>HTTPS</th>
                                <th>STATUS</th>
                              </tr>
                            </thead>
                            <tbody>
                              {data.sites
                                .filter((s) => s.present)
                                .map((s) => (
                                  <tr
                                    key={s.id}
                                    onClick={() => setSelected(s.id)}
                                    className="clickable"
                                  >
                                    <td>
                                      <div className="project-cell">
                                        <span className="project-icon">
                                          {s.project_type === "laravel"
                                            ? "L"
                                            : "P"}
                                        </span>
                                        <div>
                                          <strong>{s.hostname}</strong>
                                          <small>
                                            {s.project_type === "laravel"
                                              ? "Laravel"
                                              : "Plain PHP"}
                                          </small>
                                        </div>
                                      </div>
                                    </td>
                                    <td>{bindingLabel(s, "php")}</td>
                                    <td>{bindingLabel(s, "mysql")}</td>
                                    <td>
                                      <Badge value={s.https} />
                                    </td>
                                    <td>
                                      <Badge value={s.status} />
                                      <span className="chevron">›</span>
                                    </td>
                                  </tr>
                                ))}
                            </tbody>
                          </table>
                        )}
                      </div>
                      {data.sites.some((s) => !s.present) && (
                        <p className="footnote">
                          {data.sites.filter((s) => !s.present).length} missing
                          project record(s) retained. Project files and database
                          data are preserved.
                        </p>
                      )}
                    </>
                  ))}
                {page === "Runtimes" && (
                  <>
                    {kinds.map((kind) => (
                      <section className="panel" key={kind}>
                        <div className="panel-header">
                          <div>
                            <h2>
                              {kind === "php"
                                ? "PHP"
                                : kind === "mysql"
                                  ? "MySQL"
                                  : "Caddy"}
                            </h2>
                            <p>
                              {kind === "php"
                                ? "Version-based FastCGI pools"
                                : kind === "mysql"
                                  ? "One persistent instance per engine version"
                                  : "Local routing and automatic HTTPS"}
                            </p>
                          </div>
                          <button
                            onClick={() => setImportKind(kind)}
                            disabled={busy}
                          >
                            ＋ Import Runtime
                          </button>
                        </div>
                        <h3 className="panel-copy">Installed</h3>
                        {data.installed
                          .filter((r) => r.manifest.runtime === kind)
                          .map((r) => (
                            <div className="runtime-row" key={r.id}>
                              <div>
                                <strong>{r.manifest.version}</strong>{" "}
                                <Badge value="installed" />
                                {data.defaults[kind] === r.manifest.version && (
                                  <span className="default-tag">DEFAULT</span>
                                )}
                                <small>{r.relative_path}</small>
                              </div>
                              <div className="actions">
                                {kind === "php" && (
                                  <button onClick={() => setPhp(r)}>
                                    Configure
                                  </button>
                                )}
                                <button
                                  disabled={busy}
                                  onClick={() =>
                                    void act({
                                      type: "validate",
                                      runtime: {
                                        kind,
                                        version: r.manifest.version,
                                      },
                                    })
                                  }
                                >
                                  Health check
                                </button>
                                <button
                                  disabled={
                                    busy ||
                                    data.defaults[kind] === r.manifest.version
                                  }
                                  onClick={() =>
                                    void act({
                                      type: "default",
                                      runtime: {
                                        kind,
                                        version: r.manifest.version,
                                      },
                                    })
                                  }
                                >
                                  Set default
                                </button>
                                <button
                                  disabled={busy}
                                  onClick={() => {
                                    if (
                                      window.confirm(
                                        "Remove runtime binaries? Referenced runtimes cannot be removed.",
                                      )
                                    )
                                      void act({
                                        type: "remove",
                                        runtime: {
                                          kind,
                                          version: r.manifest.version,
                                        },
                                      });
                                  }}
                                >
                                  Remove
                                </button>
                              </div>
                            </div>
                          ))}
                        {!data.installed.some(
                          (r) => r.manifest.runtime === kind,
                        ) && (
                          <div className="empty compact">
                            No {kind} runtime installed. Import a native Windows
                            x64 distribution.
                          </div>
                        )}
                        <h3 className="panel-copy">Available</h3>
                        {data.available
                          .filter(
                            (m) =>
                              m.platform === data.platform &&
                              m.runtime === kind &&
                              !data.installed.some(
                                (r) => r.id === kind + ":" + m.version,
                              ),
                          )
                          .map((m) => (
                            <div className="runtime-row" key={m.version}>
                              <span>
                                {m.version}{" "}
                                <span className="muted">Catalog</span>
                              </span>
                              <button
                                disabled={busy || !m.download || !m.sha256}
                                onClick={() =>
                                  void act({
                                    type: "install",
                                    runtime: { kind, version: m.version },
                                  })
                                }
                              >
                                Install version
                              </button>
                            </div>
                          ))}
                      </section>
                    ))}
                    <p className="footnote">
                      Available versions come from bundled, local, or
                      integrity-verified remote catalog metadata. Node execution
                      is planned.
                    </p>
                  </>
                )}
                {page === "Databases" && (
                  <>
                    <div className="alert">
                      MySQL binds to 127.0.0.1. New development instances
                      initialize with root and an empty password; project users
                      receive database-specific grants with encrypted local
                      credential references.
                    </div>
                    <section className="panel">
                      <div className="panel-header">
                        <h2>MySQL instances</h2>
                      </div>
                      {data.installed
                        .filter(
                          (r) =>
                            r.manifest.runtime === "mysql" &&
                            !data.databases.some(
                              (db) => db.runtime_id === r.id,
                            ),
                        )
                        .map((r) => (
                          <div className="runtime-row" key={r.id}>
                            {r.id}
                            <button
                              disabled={busy}
                              onClick={() =>
                                void act({
                                  type: "database",
                                  runtime: {
                                    kind: "mysql",
                                    version: r.manifest.version,
                                  },
                                  operation: "initialize",
                                })
                              }
                            >
                              Initialize persistent instance
                            </button>
                          </div>
                        ))}
                      {data.databases.length === 0 ? (
                        <div className="empty">
                          No instance initialized. Select an installed MySQL
                          runtime for a site, then Start All.
                        </div>
                      ) : (
                        data.databases.map((db) => (
                          <div className="runtime-row" key={db.runtime_id}>
                            <div>
                              <strong>{db.runtime_id}</strong>
                              <small>{db.data_path}</small>
                            </div>
                            <span>127.0.0.1:{db.port ?? "unallocated"}</span>
                            <div className="actions">
                              {(
                                [
                                  "start",
                                  "stop",
                                  "restart",
                                  "validate",
                                ] as const
                              ).map((operation) => (
                                <button
                                  key={operation}
                                  disabled={busy}
                                  onClick={() =>
                                    void act({
                                      type: "database",
                                      runtime: {
                                        kind: "mysql",
                                        version: db.runtime_id.slice(6),
                                      },
                                      operation,
                                    })
                                  }
                                >
                                  {operation}
                                </button>
                              ))}
                            </div>
                            <Badge
                              value={
                                data.services.find(
                                  (s) => s.key === db.runtime_id,
                                )?.status ??
                                (db.initialized
                                  ? "stopped"
                                  : "initialization incomplete")
                              }
                            />
                          </div>
                        ))
                      )}
                    </section>
                    <p className="footnote">
                      Removing or moving a site never removes its database data.
                      Engine versions are not upgraded automatically.
                    </p>
                  </>
                )}
                {page === "Logs" && <Logs site={logSite} />}
                {page === "Settings" && (
                  <>
                    <button
                      disabled={busy}
                      onClick={() => {
                        setSetupDismissed(false);
                        void act({ type: "reopen_setup" });
                      }}
                    >
                      เปิด Setup wizard อีกครั้ง
                    </button>
                    <section className="panel">
                      <div className="panel-header">
                        <h2>Workspace</h2>
                      </div>
                      <dl>
                        <dt>DEVONE_HOME</dt>
                        <dd>
                          <code>{data.home}</code>
                        </dd>
                        <dt>Platform</dt>
                        <dd>{data.platform}</dd>
                        <dt>Projects</dt>
                        <dd>
                          <code>{data.home}/www</code>
                        </dd>
                        <dt>Home override</dt>
                        <dd>
                          Set DEVONE_HOME before launching. Restart required.
                        </dd>
                      </dl>
                    </section>
                    <ProductSettings data={data} busy={busy} act={act} />
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
                      </details>
                    </section>
                  </>
                )}
              </>
            )
          )}
        </div>
        <footer>
          DEVONE LOCAL <span>PHP + MySQL · Phase 1 foundation</span>
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
