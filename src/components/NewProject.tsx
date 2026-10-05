import { ErrorNotice } from "./ErrorNotice";
import { editorChoice, creationStage, initialVersions } from "../presentation";
import { useState } from "react";
import type {
  Snapshot,
  Action,
  RuntimeKind,
  CreateRequest,
} from "../contracts";
import { bridge } from "../bridge";
export function NewProject({
  data,
  busy,
  act,
  view,
}: {
  data: Snapshot;
  busy: boolean;
  act: (a: Action) => Promise<void>;
  view: (id: string) => void;
}) {
  const [name, setName] = useState("");
  const [templateId, setTemplate] = useState("static");
  const [runtimes, setRuntimes] = useState<Record<string, string>>(
    initialVersions(data),
  );
  const [tools, setTools] = useState<Record<string, string>>({
    ...Object.fromEntries(
      ["pnpm", "composer"].map((id) => [
        id,
        data.tool_defaults?.[id] ??
          data.tools?.find((t) => t.id === id)?.version ??
          data.available_tools?.find((t) => t.id === id)?.version ??
          "",
      ]),
    ),
  });
  const [node, setNode] = useState(false);
  const [database, setDatabase] = useState(false);
  const [dbName, setDbName] = useState("");
  const [install, setInstall] = useState(false);
  const [mail, setMail] = useState(false);
  const [log, setLog] = useState("");
  const [error, setError] = useState("");
  const template = data.developer?.templates.find((t) => t.id === templateId);
  const kinds = template
    ? [
        ...new Set([
          ...Object.keys(template.runtimes),
          ...(templateId === "laravel" && node ? ["node"] : []),
          ...(["laravel", "blank-php"].includes(templateId) && database
            ? ["mysql"]
            : []),
        ]),
      ]
    : [];
  const managers = template
    ? [
        ...new Set([
          ...template.tools,
          ...(templateId === "laravel" && node ? ["pnpm"] : []),
        ]),
      ]
    : [];
  const active = data.developer?.creation_tasks.some(
    (t) => t.status === "running",
  );
  const valid = /^[a-z0-9](?:[a-z0-9-]{0,61}[a-z0-9])?$/.test(name);
  const missing = kinds.filter(
    (k) =>
      !data.installed.some(
        (r) => r.manifest.runtime === k && r.manifest.version === runtimes[k],
      ),
  );
  const missingTools = managers.filter(
    (k) =>
      !(data.tools ?? []).some((t) => t.id === k && t.version === tools[k]),
  );
  const create = () => {
    if (!template) return;
    const request: CreateRequest = {
      name,
      template: template.id,
      runtimes: Object.fromEntries(kinds.map((k) => [k, runtimes[k] ?? ""])),
      tools: Object.fromEntries(managers.map((k) => [k, tools[k] ?? ""])),
      install_dependencies: install && kinds.includes("node"),
      database_name: kinds.includes("mysql")
        ? dbName || name.replaceAll("-", "_")
        : null,
      configure_mail: templateId === "laravel" && mail,
    };
    void act({ type: "create_project", request });
  };
  return (
    <>
      <section className="panel">
        <h2>New Project</h2>
        <p>
          Create in your www directory. Existing projects are still discovered
          automatically.
        </p>
        {data.developer?.template_error && (
          <p role="alert">
            Custom template error: {data.developer.template_error}
          </p>
        )}
        <div className="creation-fields">
          <label>
            Project name
            <input
              value={name}
              onChange={(e) => setName(e.target.value)}
              placeholder="my-app"
              maxLength={63}
            />
          </label>
          <fieldset className="template-picker">
            <legend>Choose a template</legend>
            <div className="template-grid">
              {data.developer?.templates.map((t) => (
                <button
                  type="button"
                  key={t.id}
                  aria-pressed={templateId === t.id}
                  disabled={!!active || busy}
                  onClick={() => {
                    setTemplate(t.id);
                    setInstall(false);
                    setMail(false);
                    setNode(false);
                    setDatabase(false);
                    setRuntimes(initialVersions(data, t));
                  }}
                >
                  <strong>{t.name}</strong>
                  <small>
                    {t.custom
                      ? "Custom code - review before use"
                      : t.id === "wordpress"
                        ? "Website with a database"
                        : t.id === "static"
                          ? "HTML, CSS and JavaScript"
                          : t.id === "laravel"
                            ? "PHP application"
                            : t.id === "blank-php"
                              ? "Simple PHP starter"
                              : "JavaScript / TypeScript app"}
                  </small>
                </button>
              ))}
            </div>
          </fieldset>
          {name && !valid && (
            <p role="alert">
              Use lowercase letters, digits and hyphens, without
              leading/trailing hyphens.
            </p>
          )}
          {templateId === "laravel" && (
            <label>
              <input
                type="checkbox"
                checked={node}
                onChange={(e) => setNode(e.target.checked)}
              />{" "}
              Include Node/frontend setup
            </label>
          )}
          {["blank-php", "laravel"].includes(templateId) && (
            <label>
              <input
                type="checkbox"
                checked={database}
                onChange={(e) => setDatabase(e.target.checked)}
              />{" "}
              Provision a project database
            </label>
          )}
          {kinds.map((k) => (
            <label key={k}>
              {k.toUpperCase()}
              {template?.runtimes[k]
                ? ` (minimum ${template.runtimes[k]})`
                : ""}
              <select
                value={runtimes[k] ?? ""}
                onChange={(e) =>
                  setRuntimes({ ...runtimes, [k]: e.target.value })
                }
              >
                <option value="">Select a version</option>
                {[
                  ...new Set([
                    ...data.installed
                      .filter((r) => r.manifest.runtime === k)
                      .map((r) => r.manifest.version),
                    ...data.available
                      .filter((r) => r.runtime === k)
                      .map((r) => r.version),
                  ]),
                ].map((v) => (
                  <option key={v} value={v}>
                    {v}
                    {data.installed.some(
                      (r) =>
                        r.manifest.runtime === k && r.manifest.version === v,
                    )
                      ? ""
                      : " · Not installed"}
                  </option>
                ))}
              </select>
            </label>
          ))}
          {missing.map((k) => (
            <div key={k} role="alert">
              {k} {runtimes[k] || "version"} is not installed.{" "}
              <button
                disabled={
                  busy ||
                  !runtimes[k] ||
                  !data.available.some(
                    (r) => r.runtime === k && r.version === runtimes[k],
                  )
                }
                onClick={() =>
                  void act({
                    type: "install",
                    runtime: { kind: k as RuntimeKind, version: runtimes[k] },
                  })
                }
              >
                Install selected {k}
              </button>
            </div>
          ))}
          {managers.map((k) => (
            <label key={k}>
              {k}
              <select
                value={tools[k] ?? ""}
                onChange={(e) => setTools({ ...tools, [k]: e.target.value })}
              >
                <option value="">Select a version</option>
                {(data.available_tools ?? [])
                  .filter((t) => t.id === k)
                  .map((t) => (
                    <option key={t.version} value={t.version}>
                      {t.version}
                      {(data.tools ?? []).some(
                        (v) => v.id === k && v.version === t.version,
                      )
                        ? ""
                        : " · Not installed"}
                    </option>
                  ))}
              </select>
            </label>
          ))}
          {missingTools.map((k) => (
            <div key={k} role="alert">
              Selected {k} is not installed.{" "}
              <button
                disabled={
                  busy || !tools[k] || !runtimes[k === "pnpm" ? "node" : "php"]
                }
                onClick={() =>
                  void act({
                    type: "install_tool",
                    id: k,
                    version: tools[k],
                    node: runtimes[k === "pnpm" ? "node" : "php"],
                  })
                }
              >
                Install selected {k}
              </button>
            </div>
          ))}
          {kinds.includes("mysql") && (
            <label>
              Database name
              <input
                value={dbName}
                onChange={(e) => setDbName(e.target.value)}
                placeholder={name.replaceAll("-", "_")}
              />
              <small>
                A database-specific user and password will be generated.
              </small>
            </label>
          )}
          {kinds.includes("node") && (
            <label>
              <input
                type="checkbox"
                checked={install}
                onChange={(e) => setInstall(e.target.checked)}
              />{" "}
              Install Node dependencies as part of creation (executes project
              install scripts)
            </label>
          )}
          {templateId === "laravel" && (
            <label>
              <input
                type="checkbox"
                checked={mail}
                disabled={!data.developer?.mail.running}
                onChange={(e) => setMail(e.target.checked)}
              />{" "}
              Configure the running Mailpit for this new project
            </label>
          )}
          {template?.custom && (
            <p role="alert">
              This custom template may run code with your account. Review its
              files and commands in Settings before creating.
            </p>
          )}
          {templateId === "laravel" && (
            <p>
              Composer creates the pinned Laravel skeleton and installs PHP
              dependencies with scripts disabled. DEVONE sets local settings and
              runs key:generate only for this new project.
            </p>
          )}
          {templateId === "wordpress" && (
            <p>
              Verified WordPress files and a project database are created. Start
              the site and finish installation in your browser.
            </p>
          )}
          <p>
            {name ? `https://${name}.test` : "Choose a name"}. Creation does not
            start project web servers or workers.
          </p>
          <button
            className="primary"
            disabled={
              busy ||
              active ||
              !valid ||
              !template ||
              missing.length > 0 ||
              missingTools.length > 0
            }
            onClick={create}
          >
            Create Project
          </button>
        </div>
      </section>
      <section className="panel">
        <h2>Project creation</h2>
        {!data.developer?.creation_tasks.length && (
          <p className="panel-copy">
            Your creation progress and results will appear here.
          </p>
        )}
        {[...(data.developer?.creation_tasks ?? [])]
          .sort((a, b) => b.created_at - a.created_at)
          .map((t) => (
            <div className="runtime-row" key={t.id}>
              <div>
                <strong>{t.project_name}</strong>
                <p>
                  {t.status === "completed"
                    ? "Project ready"
                    : t.status === "failed"
                      ? "Could not finish"
                      : t.status === "cancelled"
                        ? "Cancelled"
                        : "Creating project"}{" "}
                  - {creationStage(t.stage)}
                </p>
                <ErrorNotice error={t.error ?? ""} />
                {t.destination ? (
                  <p>
                    {t.status === "completed"
                      ? "Created at"
                      : "Project folder retained at"}{" "}
                    {t.destination}.{" "}
                    {t.status !== "completed" &&
                      "Open its details to recover dependencies; creation will not overwrite this folder."}
                  </p>
                ) : (
                  t.status !== "running" && (
                    <p>
                      No project folder was committed. Review your selections
                      before retrying.
                    </p>
                  )
                )}
                {(t.status === "failed" || t.status === "cancelled") &&
                  !t.destination && (
                    <button
                      disabled={busy || !!active}
                      onClick={() => {
                        setName(t.project_name);
                        setTemplate(t.template);
                        setRuntimes(
                          initialVersions(
                            data,
                            data.developer?.templates.find(
                              (v) => v.id === t.template,
                            ),
                          ),
                        );
                        setNode(false);
                        setDatabase(false);
                        setInstall(false);
                        setMail(false);
                        setDbName("");
                      }}
                    >
                      Retry setup (review selections)
                    </button>
                  )}
              </div>
              <div className="actions">
                {t.status === "running" && (
                  <button
                    disabled={busy}
                    onClick={() => {
                      if (
                        t.destination &&
                        !window.confirm(
                          "Cancel creation? The project folder will be kept. Dependencies or database configuration may be incomplete.",
                        )
                      )
                        return;
                      void act({ type: "cancel_creation", task_id: t.id });
                    }}
                  >
                    Cancel creation
                  </button>
                )}
                <button
                  onClick={() =>
                    void bridge
                      .readLog(t.log.split(/[\\/]/).pop()!)
                      .then(setLog)
                      .catch((e) => setError(String(e)))
                  }
                >
                  Logs
                </button>
                {t.site_id && t.status !== "running" && (
                  <>
                    <button onClick={() => view(t.site_id!)}>
                      View Project
                    </button>
                    <button
                      disabled={busy}
                      onClick={() =>
                        void act({
                          type: "site_action",
                          site_id: t.site_id!,
                          operation: "start",
                        })
                      }
                    >
                      Start Site
                    </button>
                    <button
                      disabled={busy}
                      onClick={() =>
                        void act({ type: "open_site", site_id: t.site_id! })
                      }
                    >
                      Open Site
                    </button>
                    {editorChoice(data).selected && (
                      <button
                        disabled={busy}
                        onClick={() =>
                          void act({
                            type: "editor",
                            site_id: t.site_id!,
                            editor_id: editorChoice(data).selected!.id,
                            operation: "open",
                          })
                        }
                      >
                        {editorChoice(data).label}
                      </button>
                    )}
                    <button
                      disabled={busy}
                      onClick={() =>
                        void act({ type: "terminal", site_id: t.site_id! })
                      }
                    >
                      Terminal
                    </button>
                  </>
                )}
              </div>
            </div>
          ))}
        <ErrorNotice error={error} />
        {log && <pre className="task-log">{log}</pre>}
      </section>
    </>
  );
}
