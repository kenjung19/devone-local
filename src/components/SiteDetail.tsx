import { useState } from "react";
import type { Site, Snapshot, Action, ProcessDefinition } from "../contracts";
import { Provision } from "./Provision";
import { Badge } from "./Badge";
export function Stat({ label, value }: { label: string; value: string }) {
  return (
    <div className="stat">
      <span>{label}</span>
      <strong>{value}</strong>
    </div>
  );
}
export function SiteDetail({
  site,
  data,
  busy,
  act,
  back,
  logs,
}: {
  site: Site;
  data: Snapshot;
  busy: boolean;
  act: (a: Action) => Promise<void>;
  back: () => void;
  logs: () => void;
}) {
  const [custom, setCustom] = useState(false);
  const [definition, setDefinition] = useState<ProcessDefinition>({
    id: "worker",
    name: "Worker",
    runtime: "node",
    executable: "node",
    args: ["worker.js"],
    cwd: ".",
    env: {},
    port: false,
    autostart: false,
  });
  const [argumentsText, setArgumentsText] = useState("worker.js");
  const kinds = (["php", "mysql", "node"] as const).filter(
    (kind) =>
      kind in site.resolved ||
      site.metadata?.requirements.includes(kind) ||
      (!site.metadata && kind !== "node"),
  );
  const missing = kinds.flatMap((kind) => {
    const version = site.resolved[kind];
    return version &&
      !data.installed.some(
        (r) => r.manifest.runtime === kind && r.manifest.version === version,
      )
      ? [{ kind, version }]
      : [];
  });
  return (
    <>
      <button className="back" onClick={back}>
        ← All sites
      </button>
      {missing.map(({ kind, version }) => (
        <div className="alert error" role="alert" key={kind}>
          <strong>
            {site.hostname} requires {kind.toUpperCase()} {version}
          </strong>
          <p>
            {kind.toUpperCase()} {version} is not installed. This explicit
            version is retained.
          </p>
          {data.available.some(
            (m) =>
              m.runtime === kind &&
              m.version === version &&
              m.platform === data.platform &&
              m.download &&
              m.sha256,
          ) && (
            <button
              disabled={busy}
              onClick={() =>
                void act({ type: "install", runtime: { kind, version } })
              }
            >
              Install {kind.toUpperCase()} {version}
            </button>
          )}
          <button
            disabled={busy}
            onClick={() => document.getElementById(`runtime-${kind}`)?.focus()}
          >
            Change Runtime
          </button>
        </div>
      ))}
      <section className="panel">
        <div className="panel-header">
          <div>
            <h2>{site.hostname}</h2>
            <p>{site.project_type.toUpperCase()}</p>
          </div>
          <Badge value={site.status} />
        </div>
        <dl>
          <dt>Project path</dt>
          <dd>
            <code>{site.project_path}</code>
          </dd>
          <dt>Document root</dt>
          <dd>
            <code>{site.document_root}</code>
          </dd>
          {kinds.map((kind) => (
            <div className="dl-row" key={kind}>
              <dt>{kind.toUpperCase()}</dt>
              <dd>
                <select
                  id={`runtime-${kind}`}
                  disabled={busy}
                  value={site.overrides[kind] ?? ""}
                  onChange={(e) =>
                    void act({
                      type: "override",
                      site_id: site.id,
                      kind,
                      version: e.target.value || null,
                    })
                  }
                >
                  <option value="">
                    Global default ({data.defaults[kind] ?? "not configured"})
                  </option>
                  {missing.some((m) => m.kind === kind) &&
                    site.overrides[kind] && (
                      <option value={site.overrides[kind]} disabled>
                        {site.overrides[kind]} · Not installed
                      </option>
                    )}
                  {data.installed
                    .filter((r) => r.manifest.runtime === kind)
                    .map((r) => (
                      <option key={r.id} value={r.manifest.version}>
                        {r.manifest.version}
                      </option>
                    ))}
                </select>
              </dd>
            </div>
          ))}
          {kinds.map((kind) => (
            <div className="dl-row" key={`effective-${kind}`}>
              <dt>Effective {kind.toUpperCase()}</dt>
              <dd>
                {site.overrides[kind] ? "Site override" : "Global default"} ·{" "}
                {site.resolved[kind] ?? "unconfigured"}
              </dd>
            </div>
          ))}
          <dt>DNS</dt>
          <dd>{data.dns_ready ? "Wildcard .test ready" : "Setup required"}</dd>
          <dt>HTTPS</dt>
          <dd>
            <Badge value={site.https} />
          </dd>
        </dl>
        <div className="panel-buttons">
          <button onClick={logs}>Logs</button>
          <button
            className="primary"
            disabled={busy || site.status !== "running" || !data.dns_ready}
            onClick={() => void act({ type: "open_site", site_id: site.id })}
          >
            Open Site ↗
          </button>
          <button
            disabled={busy}
            onClick={() => void act({ type: "open_folder", site_id: site.id })}
          >
            Open Folder
          </button>
          <button
            disabled={busy}
            onClick={() => void act({ type: "terminal", site_id: site.id })}
          >
            Terminal
          </button>
        </div>
      </section>
      {kinds.includes("mysql") && (
        <Provision site={site} data={data} busy={busy} act={act} />
      )}
      <section className="panel">
        <div className="panel-header">
          <h2>Project processes</h2>
          <button disabled={busy} onClick={() => setCustom(!custom)}>
            Add custom process
          </button>
        </div>
        <p>
          Processes start only after explicit Start. Saved autostart applies
          after a process has been enabled.
        </p>
        <div className="panel-buttons">
          {(["start", "stop", "restart"] as const).map((operation) => (
            <button
              key={operation}
              disabled={busy}
              onClick={() =>
                void act({ type: "site_action", site_id: site.id, operation })
              }
            >
              {operation} site
            </button>
          ))}
        </div>
        {(site.processes ?? []).map((p) => {
          const state = data.services.find((s) => s.key === p.key);
          return (
            <div className="stat" key={p.key}>
              <strong>{p.definition.name}</strong>
              <code>
                {p.definition.executable} {p.definition.args.join(" ")}
              </code>
              <span>
                {state?.status ?? "stopped"} ·{" "}
                {p.enabled ? "enabled" : "disabled"} ·{" "}
                {p.definition.autostart ? "autostart" : "manual"}
                {state?.port ? ` · Port ${state.port}` : ""}
              </span>
              <div className="panel-buttons">
                {(["start", "stop", "restart"] as const).map((operation) => (
                  <button
                    key={operation}
                    disabled={busy}
                    onClick={() =>
                      void act({
                        type: "site_process",
                        site_id: site.id,
                        process_id: p.definition.id,
                        operation,
                      })
                    }
                  >
                    {operation}
                  </button>
                ))}
                <button onClick={logs}>Logs</button>
                <button
                  disabled={busy}
                  onClick={() => {
                    setDefinition(p.definition);
                    setArgumentsText(p.definition.args.join("\n"));
                    setCustom(true);
                  }}
                >
                  Edit process
                </button>
                <button
                  disabled={busy}
                  onClick={() =>
                    void act({
                      type: "site_process",
                      site_id: site.id,
                      process_id: p.definition.id,
                      operation: "remove",
                    })
                  }
                >
                  {["web", "vite", "queue", "scheduler"].includes(
                    p.definition.id,
                  )
                    ? "Reset definition"
                    : "Remove process"}
                </button>
              </div>
            </div>
          );
        })}
        {custom && (
          <form
            onSubmit={(e) => {
              e.preventDefault();
              void act({
                type: "save_process",
                site_id: site.id,
                definition: {
                  ...definition,
                  args: argumentsText.split("\n").filter((a) => a.length > 0),
                },
              });
            }}
          >
            <label>
              Process ID
              <input
                required
                value={definition.id}
                onChange={(e) =>
                  setDefinition({ ...definition, id: e.target.value })
                }
              />
            </label>
            <label>
              Name
              <input
                required
                value={definition.name}
                onChange={(e) =>
                  setDefinition({ ...definition, name: e.target.value })
                }
              />
            </label>
            <label>
              Managed executable
              <select
                value={definition.executable}
                onChange={(e) =>
                  setDefinition({
                    ...definition,
                    executable: e.target.value,
                    runtime: ["php", "composer"].includes(e.target.value)
                      ? "php"
                      : "node",
                  })
                }
              >
                {["node", "pnpm", "php", "composer"].map((executable) => (
                  <option key={executable}>{executable}</option>
                ))}
              </select>
            </label>
            <label>
              Arguments (one argument per line)
              <textarea
                rows={4}
                value={argumentsText}
                onChange={(e) => setArgumentsText(e.target.value)}
              />
            </label>
            <label>
              Working directory (relative to project)
              <input
                required
                value={definition.cwd}
                onChange={(e) =>
                  setDefinition({ ...definition, cwd: e.target.value })
                }
              />
            </label>
            <label>
              <input
                type="checkbox"
                checked={definition.port}
                onChange={(e) =>
                  setDefinition({ ...definition, port: e.target.checked })
                }
              />
              Reserve a managed port and require an owned listener
            </label>
            <label>
              <input
                type="checkbox"
                checked={definition.autostart}
                onChange={(e) =>
                  setDefinition({ ...definition, autostart: e.target.checked })
                }
              />
              Autostart after explicit first Start
            </label>
            <button disabled={busy}>Save disabled process</button>
          </form>
        )}
        <button
          disabled={busy}
          onClick={() => void act({ type: "save_portable", site_id: site.id })}
        >
          Save .devone.json
        </button>
      </section>
      {(site.metadata?.package_manager ||
        kinds.includes("node") ||
        site.project_type === "laravel" ||
        site.metadata?.composer_manifest) && (
        <section className="panel">
          <h2>Dependencies & tools</h2>
          {kinds.includes("node") && (
            <>
              <p>
                Package manager:{" "}
                {site.metadata?.package_manager ?? "not declared"}
                {site.metadata?.package_manager_version
                  ? ` ${site.metadata.package_manager_version}`
                  : ""}{" "}
                ·{" "}
                {site.metadata?.node_dependencies
                  ? "node_modules present"
                  : "node_modules missing"}
              </p>
              <button
                disabled={
                  busy ||
                  (!!site.metadata?.package_manager &&
                    site.metadata.package_manager !== "pnpm")
                }
                onClick={() =>
                  void act({
                    type: "install_dependencies",
                    site_id: site.id,
                    manager: "pnpm",
                  })
                }
              >
                Install pnpm dependencies
              </button>
            </>
          )}
          {(site.project_type === "laravel" ||
            site.metadata?.composer_manifest) && (
            <>
              <p>
                {site.metadata?.composer_dependencies
                  ? "vendor present"
                  : "vendor missing"}
              </p>
              <button
                disabled={busy}
                onClick={() =>
                  void act({
                    type: "install_dependencies",
                    site_id: site.id,
                    manager: "composer",
                  })
                }
              >
                Install Composer dependencies
              </button>
            </>
          )}
          {(data.available_tools ?? [])
            .filter(
              (t) =>
                (t.id === "pnpm" && kinds.includes("node")) ||
                (t.id === "composer" && kinds.includes("php")),
            )
            .map((t) => (
              <p key={t.id}>
                {t.id} {t.version}:{" "}
                {(data.tools ?? []).some(
                  (v) => v.id === t.id && v.version === t.version,
                ) ? (
                  "managed tool installed"
                ) : (
                  <button
                    disabled={busy}
                    onClick={() =>
                      void act({
                        type: "install_tool",
                        id: t.id,
                        version: t.version,
                        node: site.resolved.node ?? null,
                      })
                    }
                  >
                    Install managed {t.id}
                  </button>
                )}
              </p>
            ))}
          {site.metadata?.error && (
            <p className="alert error">{site.metadata.error}</p>
          )}
        </section>
      )}
      {site.issue && <div className="alert error">{site.issue}</div>}
      <p className="footnote">
        Overrides are persisted independently of global defaults. Terminal PATH
        applies only to the new terminal session.
      </p>
    </>
  );
}
