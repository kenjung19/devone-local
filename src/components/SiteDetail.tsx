import { editorChoice, siteStatus, siteProblems } from "../presentation";
import { ErrorNotice } from "./ErrorNotice";
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
  act: (a: Action) => Promise<boolean>;
  back: () => void;
  logs: () => void;
}) {
  const [custom, setCustom] = useState(false);
  const [editing, setEditing] = useState(false);
  const [environmentText, setEnvironmentText] = useState("");
  const [validationError, setValidationError] = useState("");
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
      (!site.metadata &&
        ["php", "plain_php", "laravel"].includes(site.project_type) &&
        kind !== "node"),
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
      <div className="actions">
        <button
          className="primary"
          disabled={busy}
          onClick={() =>
            void act({
              type: "site_action",
              site_id: site.id,
              operation: ["running", "starting"].includes(site.status)
                ? "stop"
                : "start",
            })
          }
        >
          {["running", "starting"].includes(site.status)
            ? "Stop Site"
            : "Start Site"}
        </button>
        {editorChoice(data).selected && (
          <button
            disabled={busy}
            onClick={() =>
              void act({
                type: "editor",
                site_id: site.id,
                editor_id: editorChoice(data).selected!.id,
                operation: "open",
              })
            }
          >
            {editorChoice(data).label}
          </button>
        )}
        {(data.developer?.editors.filter((e) => e.category === "editor")
          .length ?? 0) > 1 && (
          <details>
            <summary>Open in…</summary>
            {data.developer?.editors
              .filter((e) => e.category === "editor")
              .map((e) => (
                <button
                  key={e.id}
                  disabled={busy}
                  onClick={() =>
                    void act({
                      type: "editor",
                      site_id: site.id,
                      editor_id: e.id,
                      operation: "open",
                    })
                  }
                >
                  {e.name}
                </button>
              ))}
          </details>
        )}
        <button
          disabled={busy}
          onClick={() =>
            void act({
              type: "site_preference",
              site_id: site.id,
              operation: data.developer?.preferences[site.id]?.favorite
                ? "unfavorite"
                : "favorite",
            })
          }
        >
          {data.developer?.preferences[site.id]?.favorite
            ? "Unpin"
            : "Pin Site"}
        </button>
      </div>
      {data.developer?.mail.running &&
        ["php", "laravel", "wordpress", "plain_php"].includes(
          site.project_type,
        ) && (
          <section className="panel">
            <h2>Local mail suggestion</h2>
            <p>Suggested values only; existing project files are not edited.</p>
            <pre>{`MAIL_MAILER=smtp\nMAIL_HOST=127.0.0.1\nMAIL_PORT=${data.developer.mail.smtp_port}`}</pre>
          </section>
        )}
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
      <ErrorNotice error={site.metadata?.error ?? ""} />
      {siteProblems(site, data)
        .filter((p) => !p.message.includes("not installed"))
        .map((p, i) => (
          <div className="alert" key={i}>
            {p.message}
            {p.action && (
              <button disabled={busy} onClick={() => void act(p.action!)}>
                {p.label}
              </button>
            )}
          </div>
        ))}
      <section className="panel">
        <div className="panel-header">
          <div>
            <h2>{site.hostname}</h2>
            <p>{site.project_type.toUpperCase()}</p>
          </div>
          <Badge value={siteStatus(site, data)} />
        </div>
        <dl>
          <dt>Project path</dt>
          <dd>
            <code>{site.project_path}</code>
          </dd>
          <dt>Web folder</dt>
          <dd>
            <code>{site.document_root}</code>
          </dd>
          {kinds.map((kind) => (
            <div className="dl-row" key={kind}>
              <dt>{kind.toUpperCase()}</dt>
              <dd>
                <select
                  aria-label={`${kind.toUpperCase()} version`}
                  id={`runtime-${kind}`}
                  disabled={busy}
                  value={(site.local_overrides ?? site.overrides)[kind] ?? ""}
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
                    {site.metadata?.runtimes[kind]
                      ? `Portable config (${site.metadata.runtimes[kind]})`
                      : `Global default (${data.defaults[kind] ?? "not configured"})`}
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
                {site.runtime_sources?.[kind] ??
                  (site.overrides[kind]
                    ? "Local override"
                    : "Global default")}{" "}
                · {site.resolved[kind] ?? "unconfigured"}
              </dd>
            </div>
          ))}
          <dt>Local domains</dt>
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
      <details className="panel panel-copy">
        <summary>Advanced project processes</summary>
        <section>
          <div className="panel-header">
            <h2>Project processes</h2>
            <button
              disabled={busy}
              onClick={() => {
                setCustom(!custom);
                setEditing(false);
                setEnvironmentText("");
                setDefinition({ ...definition, id: "worker", name: "Worker" });
              }}
            >
              Add custom process
            </button>
          </div>
          <p>
            Processes start only after explicit Start. Saved autostart applies
            after a process has been enabled.
          </p>
          {site.project_type === "node" &&
            (site.metadata?.scripts ?? []).map((script) => (
              <button
                key={script}
                disabled={busy}
                onClick={() => {
                  const replace = (site.processes ?? []).some(
                    (p) => p.definition.id === "web",
                  );
                  if (
                    replace &&
                    !window.confirm(
                      `Replace the web process definition with the ${script} script? The current process will stop.`,
                    )
                  )
                    return;
                  void act({
                    type: "save_process",
                    site_id: site.id,
                    replace,
                    definition: {
                      id: "web",
                      name: "Web server",
                      runtime: "node",
                      executable: "pnpm",
                      args: ["run", script],
                      cwd: ".",
                      env: {},
                      port: true,
                      autostart: true,
                    },
                  });
                }}
              >
                Use {script} script (Local override)
              </button>
            ))}
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
          {(["Web", "Frontend", "Workers", "Custom"] as const).map((group) => (
            <div key={group}>
              <h3>{group}</h3>
              {group === "Web" && site.metadata?.route === "php_fastcgi" && (
                <p>
                  PHP ·{" "}
                  {data.services.find(
                    (s) => s.key === `php:${site.resolved.php}`,
                  )?.status ?? "stopped"}
                </p>
              )}
              {(site.processes ?? [])
                .filter(
                  (p) =>
                    (p.definition.id === "web"
                      ? "Web"
                      : p.definition.id === "vite"
                        ? "Frontend"
                        : ["queue", "scheduler"].includes(p.definition.id)
                          ? "Workers"
                          : "Custom") === group,
                )
                .map((p) => {
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
                        {(["start", "stop", "restart"] as const).map(
                          (operation) => (
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
                          ),
                        )}
                        <button
                          disabled={busy}
                          onClick={() =>
                            void act({
                              type: "site_process",
                              site_id: site.id,
                              process_id: p.definition.id,
                              operation: p.definition.autostart
                                ? "disable_autostart"
                                : "enable_autostart",
                            })
                          }
                        >
                          {p.definition.autostart
                            ? "Disable Autostart"
                            : "Enable Autostart"}
                        </button>
                        <button
                          disabled={busy || !p.enabled}
                          onClick={() =>
                            void act({
                              type: "site_process",
                              site_id: site.id,
                              process_id: p.definition.id,
                              operation: "disable",
                            })
                          }
                        >
                          Disable process
                        </button>
                        <button onClick={logs}>Logs</button>
                        <button
                          disabled={busy}
                          onClick={() => {
                            setEditing(true);
                            setEnvironmentText(
                              Object.entries(p.definition.env)
                                .map(([k, v]) => `${k}=${v}`)
                                .join("\n"),
                            );
                            setDefinition(p.definition);
                            setArgumentsText(p.definition.args.join("\n"));
                            setCustom(true);
                          }}
                        >
                          Edit process
                        </button>
                        <button
                          disabled={busy}
                          onClick={() => {
                            if (
                              !window.confirm(
                                `Remove the local ${p.definition.name} definition and stop its process? Detected processes will reset to their detected definition.`,
                              )
                            )
                              return;
                            void act({
                              type: "site_process",
                              site_id: site.id,
                              process_id: p.definition.id,
                              operation: "remove",
                            });
                          }}
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
            </div>
          ))}
          {custom && (
            <form
              onSubmit={(e) => {
                e.preventDefault();
                const env: Record<string, string> = {};
                for (const line of environmentText
                  .split("\n")
                  .filter(Boolean)) {
                  const index = line.indexOf("=");
                  const key = line.slice(0, index);
                  if (
                    index < 1 ||
                    !/^[A-Za-z_][A-Za-z0-9_]*$/.test(key) ||
                    /^(PATH|PORT|HOST|PHPRC|DEVONE_HOME|DEVONE_SITE|NODE_OPTIONS|COMSPEC|PHP_INI_SCAN_DIR|COREPACK_ENABLE_NETWORK)$/i.test(
                      key,
                    ) ||
                    /SECRET|TOKEN|PASSWORD|PRIVATE|KEY/i.test(key)
                  ) {
                    setValidationError(
                      "Invalid or reserved environment key. Portable config cannot contain secrets.",
                    );
                    return;
                  }
                  env[key] = line.slice(index + 1);
                }
                if (
                  !editing &&
                  (site.processes ?? []).some(
                    (p) => p.definition.id === definition.id,
                  )
                ) {
                  setValidationError("Process ID already exists. Use Edit.");
                  return;
                }
                setValidationError("");
                void act({
                  type: "save_process",
                  replace: editing,
                  site_id: site.id,
                  definition: {
                    ...definition,
                    env,
                    args: argumentsText.split("\n").filter((a) => a.length > 0),
                  },
                });
              }}
            >
              <label>
                Process ID
                <input
                  required
                  disabled={editing}
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
                Health strategy
                <select
                  value={definition.health ?? ""}
                  onChange={(e) =>
                    setDefinition({
                      ...definition,
                      health: (e.target.value ||
                        null) as ProcessDefinition["health"],
                    })
                  }
                >
                  <option value="">
                    Automatic (HTTP for port, alive for worker)
                  </option>
                  <option value="process_alive">Process alive</option>
                  <option value="tcp_listener">Owned TCP listener</option>
                  <option value="http">HTTP response</option>
                </select>
              </label>
              <label>
                <input
                  type="checkbox"
                  checked={definition.autostart}
                  onChange={(e) =>
                    setDefinition({
                      ...definition,
                      autostart: e.target.checked,
                    })
                  }
                />
                Autostart after explicit first Start
              </label>
              <label>
                Environment (machine-local, KEY=value per line)
                <textarea
                  rows={3}
                  value={environmentText}
                  onChange={(e) => setEnvironmentText(e.target.value)}
                />
              </label>
              {validationError && (
                <p role="alert" className="alert error">
                  {validationError}
                </p>
              )}
              <p>
                Save creates a Local override. Runtime, tool and working
                directory are validated before saving.
              </p>
              <button disabled={busy}>Save disabled process</button>
            </form>
          )}
          <button
            disabled={busy}
            onClick={() =>
              void act({ type: "save_portable", site_id: site.id })
            }
          >
            Save portable config (.devone.json)
          </button>
        </section>
      </details>
      {(site.metadata?.package_manager ||
        kinds.includes("node") ||
        site.project_type === "laravel" ||
        site.metadata?.composer_manifest) && (
        <section className="panel">
          <h2>Dependencies & tools</h2>
          {site.metadata?.package_manager === "pnpm" &&
            site.metadata.package_manager_version &&
            !(data.tools ?? []).some(
              (t) =>
                t.id === "pnpm" &&
                t.version === site.metadata?.package_manager_version,
            ) && (
              <p role="alert" className="alert error">
                Required pnpm {site.metadata.package_manager_version} is not
                installed.{" "}
                {!(data.available_tools ?? []).some(
                  (t) =>
                    t.id === "pnpm" &&
                    t.version === site.metadata?.package_manager_version,
                )
                  ? "This exact version is not in the pinned tools catalog."
                  : "Use its exact install action below."}
              </p>
            )}
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
              <p>
                Dependency state:{" "}
                {site.metadata?.node_dependency_state ?? "unknown"}
              </p>
              {site.metadata?.package_manager &&
                site.metadata.package_manager !== "pnpm" && (
                  <p>
                    Automated install is not managed yet. Open site terminal to
                    continue.
                  </p>
                )}
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
              <p>
                Dependency state:{" "}
                {site.metadata?.composer_dependency_state ?? "unknown"}
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
              <p key={`${t.id}:${t.version}`}>
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
                        node:
                          (t.id === "composer"
                            ? site.resolved.php
                            : site.resolved.node) ?? null,
                      })
                    }
                  >
                    Install managed {t.id}
                  </button>
                )}
              </p>
            ))}
          {(data.dependency_tasks ?? [])
            .filter((t) => t.site_id === site.id)
            .map((t) => (
              <div key={t.manager}>
                <strong>
                  {t.manager} install · {t.status}
                </strong>
                {t.error && <p className="alert error">{t.error}</p>}
                <button onClick={logs}>Installation logs</button>
                {t.status === "running" && (
                  <button
                    onClick={() =>
                      void act({
                        type: "cancel_dependencies",
                        site_id: site.id,
                      })
                    }
                  >
                    Cancel installation
                  </button>
                )}
              </div>
            ))}
        </section>
      )}
      <ErrorNotice error={site.issue ?? ""} />
      <p className="footnote">
        Overrides are persisted independently of global defaults. Terminal PATH
        applies only to the new terminal session.
      </p>
    </>
  );
}
