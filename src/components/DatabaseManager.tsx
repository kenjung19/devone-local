import { ErrorNotice } from "./ErrorNotice";
import { useState } from "react";
import type { Snapshot, Action } from "../contracts";
import { bridge } from "../bridge";
export function DatabaseManager({
  data,
  busy,
  act,
  logs,
}: {
  data: Snapshot;
  busy: boolean;
  act: (a: Action) => Promise<void>;
  logs: () => void;
}) {
  const [runtime, setRuntime] = useState(
    data.defaults.mysql ? `mysql:${data.defaults.mysql}` : "",
  );
  const [name, setName] = useState("");
  const [user, setUser] = useState("");
  const [target, setTarget] = useState<{
    runtime: string;
    name: string;
    operation: "delete" | "restore";
  } | null>(null);
  const [confirmation, setConfirmation] = useState("");
  const [file, setFile] = useState("");
  const [error, setError] = useState("");
  const admin = (
    runtime_id: string,
    database_name: string,
    operation:
      "list" | "create" | "delete" | "backup" | "restore" | "connection",
    extra: Partial<Action & { type: "db_admin" }> = {},
  ) =>
    void act({
      type: "db_admin",
      runtime_id,
      database_name,
      operation,
      username: null,
      confirmation: null,
      path: null,
      ...extra,
    });
  const installed = data.installed.filter(
    (r) => r.manifest.runtime === "mysql",
  );
  const chooseFile = async () => {
    try {
      const path = await bridge.selectSqlFile();
      if (path) setFile(path);
    } catch (e) {
      setError(String(e));
    }
  };
  return (
    <>
      <section className="panel">
        <h2>MySQL</h2>
        {!installed.length && (
          <p className="panel-copy">
            Install MySQL from Runtimes to use local databases.
          </p>
        )}
        {installed.map((r) => {
          const db = data.databases.find((d) => d.runtime_id === r.id);
          const state = data.services.find((s) => s.key === r.id);
          return (
            <div className="runtime-row" key={r.id}>
              <div>
                <strong>MySQL {r.manifest.version}</strong>
                <p>
                  {!db?.initialized
                    ? "Needs initialization"
                    : state?.healthy
                      ? "Running"
                      : "Stopped"}{" "}
                  · 127.0.0.1:
                  {db?.port ?? "not allocated"}
                </p>
                <small>
                  {db
                    ? `${data.home}/${db.data_path}`
                    : "Data directory will be initialized explicitly"}
                </small>
              </div>
              <div className="actions">
                {(
                  [
                    "initialize",
                    "start",
                    "stop",
                    "restart",
                    "validate",
                  ] as const
                ).map((operation) => (
                  <button
                    key={operation}
                    disabled={
                      busy ||
                      (operation === "initialize" && !!db?.initialized) ||
                      (["start", "restart", "validate"].includes(operation) &&
                        !db?.initialized) ||
                      (operation === "stop" && !state?.healthy)
                    }
                    onClick={() =>
                      void act({
                        type: "database",
                        runtime: { kind: "mysql", version: r.manifest.version },
                        operation,
                      })
                    }
                  >
                    {operation}
                  </button>
                ))}
                <button
                  disabled={busy || !state?.healthy}
                  onClick={() => admin(r.id, "", "list")}
                >
                  Refresh databases
                </button>
                <button onClick={logs}>Open Logs</button>
              </div>
              {data.developer?.database_catalog[r.id] && (
                <div>
                  <p>
                    Last queried server metadata. Refresh while the instance is
                    running.
                  </p>
                  <h3>User databases</h3>
                  {data.developer.database_catalog[r.id]
                    .filter((d) => !d.system)
                    .map((d) => (
                      <p key={d.name}>
                        {d.name} ·{" "}
                        {d.managed ? "DEVONE managed" : "External · read only"}
                      </p>
                    ))}
                  <details>
                    <summary>System databases · protected</summary>
                    {data.developer.database_catalog[r.id]
                      .filter((d) => d.system)
                      .map((d) => (
                        <p key={d.name}>{d.name}</p>
                      ))}
                  </details>
                </div>
              )}
            </div>
          );
        })}
      </section>
      <section className="panel">
        <h2>Create Database</h2>
        <div className="creation-fields">
          <label>
            MySQL version
            <select
              value={runtime}
              onChange={(e) => setRuntime(e.target.value)}
            >
              <option value="">Choose a MySQL version</option>
              {installed.map((r) => (
                <option key={r.id} value={r.id}>
                  MySQL {r.manifest.version}
                </option>
              ))}
            </select>
          </label>
          <label>
            Database name
            <input
              value={name}
              onChange={(e) => setName(e.target.value)}
              placeholder="my_project"
            />
          </label>
          <label>
            Database-specific username (optional)
            <input
              value={user}
              onChange={(e) => setUser(e.target.value)}
              placeholder="Generate a new user"
            />
          </label>
          <p>
            A new user with database-specific grants and a protected generated
            password is always created. Start the selected instance first.
          </p>
          <button
            disabled={
              busy ||
              !runtime ||
              !name ||
              !data.services.some((s) => s.key === runtime && s.healthy)
            }
            onClick={() =>
              admin(runtime, name, "create", { username: user || null })
            }
          >
            Create Database and User
          </button>
        </div>
      </section>
      <section className="panel">
        <h2>Managed databases</h2>
        {!data.developer?.managed_databases.length && (
          <p className="panel-copy">
            No managed databases yet. Create one above or with a new project.
          </p>
        )}
        {data.developer?.managed_databases.map((b) => (
          <div
            className="runtime-row"
            key={`${b.runtime_id}:${b.database_name}`}
          >
            <div>
              <strong>{b.database_name}</strong>
              <p>
                MySQL {b.runtime_id.replace("mysql:", "")} · {b.status} · User{" "}
                {b.username}
              </p>
              <small>
                Last backup:{" "}
                {(() => {
                  const last = data.developer?.backups
                    .filter(
                      (v) =>
                        v.database === b.database_name &&
                        v.runtime === b.runtime_id &&
                        v.status === "completed",
                    )
                    .sort((a, b) => b.created_at - a.created_at)[0];
                  return last
                    ? new Date(last.created_at * 1000).toLocaleString()
                    : "No successful backup yet";
                })()}
              </small>
            </div>
            <div className="actions">
              <button
                disabled={
                  busy ||
                  !data.services.some(
                    (s) => s.key === b.runtime_id && s.healthy,
                  )
                }
                onClick={() => admin(b.runtime_id, b.database_name, "backup")}
              >
                Backup Now
              </button>
            </div>
            <details className="row-details">
              <summary>Connection, restore and delete</summary>
              <div className="actions">
                <button
                  disabled={busy}
                  onClick={() => {
                    setTarget({
                      runtime: b.runtime_id,
                      name: b.database_name,
                      operation: "restore",
                    });
                    setConfirmation("");
                    setFile("");
                  }}
                >
                  Restore…
                </button>
                <button
                  disabled={busy}
                  onClick={() => {
                    setTarget({
                      runtime: b.runtime_id,
                      name: b.database_name,
                      operation: "delete",
                    });
                    setConfirmation("");
                  }}
                >
                  Delete…
                </button>
                <button
                  disabled={busy}
                  onClick={() =>
                    void act({
                      type: "db_admin",
                      runtime_id: b.runtime_id,
                      operation: "credential",
                      database_name: b.database_name,
                      username: null,
                      confirmation: null,
                      path: null,
                    })
                  }
                >
                  Reveal Password
                </button>
                <button
                  onClick={() => {
                    const port = data.databases.find(
                      (d) => d.runtime_id === b.runtime_id,
                    )?.port;
                    void navigator.clipboard
                      .writeText(
                        `Host: 127.0.0.1\nPort: ${port ?? "not allocated"}\nDatabase: ${b.database_name}\nUser: ${b.username}\nPassword omitted`,
                      )
                      .catch((e) => setError(String(e)));
                  }}
                >
                  Copy Connection Info
                </button>
                {data.developer?.editors
                  .filter((e) => e.category === "database_client")
                  .map((e) => (
                    <button
                      key={e.id}
                      disabled={busy}
                      onClick={() =>
                        void act({
                          type: "editor",
                          site_id: null,
                          editor_id: e.id,
                          operation: "open",
                        })
                      }
                    >
                      Open in {e.name}
                    </button>
                  ))}
              </div>
            </details>
          </div>
        ))}
        <p>
          Opening external clients passes no passwords. Existing unrelated and
          system databases have no destructive actions.
        </p>
        {target && (
          <section
            className="confirm-panel"
            aria-labelledby="database-confirm-title"
          >
            <h3 id="database-confirm-title">
              {target.operation === "delete" ? "Delete" : "Restore into"}{" "}
              {target.name}
            </h3>
            <p role="alert">
              {target.operation === "delete"
                ? "This permanently deletes this managed database and its managed user. Backups are retained."
                : "Only choose SQL you trust. DEVONE backups replace dumped tables and include routines, events and triggers. Restoring may overwrite existing data; errors may leave partial changes. Back up first and stop the project."}
            </p>
            {target.operation === "restore" && (
              <label>
                SQL file
                <div className="actions">
                  <input
                    aria-label="Selected SQL file"
                    value={file}
                    readOnly
                    placeholder="No file selected"
                  />
                  <button disabled={busy} onClick={() => void chooseFile()}>
                    Select SQL File
                  </button>
                </div>
              </label>
            )}
            <label>
              Type {target.name} to confirm
              <input
                value={confirmation}
                onChange={(e) => setConfirmation(e.target.value)}
              />
            </label>
            <div className="actions">
              <button onClick={() => setTarget(null)}>Cancel</button>
              <button
                disabled={
                  busy ||
                  confirmation !== target.name ||
                  (target.operation === "restore" && !file)
                }
                onClick={() => {
                  admin(target.runtime, target.name, target.operation, {
                    confirmation,
                    path: file || null,
                  });
                  setTarget(null);
                }}
              >
                Confirm {target.operation}
              </button>
            </div>
          </section>
        )}
        <ErrorNotice error={error} />
      </section>
      <details className="panel panel-copy">
        <summary>Backup history</summary>
        {!data.developer?.backups.length && (
          <p>No backups yet. Use Backup Now on a managed database.</p>
        )}
        {data.developer?.backups.map((b) => (
          <div className="stat" key={b.id}>
            <strong>
              {b.database} · {b.runtime}
            </strong>
            <p>
              {b.status} · {b.size.toLocaleString()} bytes ·{" "}
              {new Date(b.created_at * 1000).toLocaleString()}
            </p>
            <small>{b.path}</small>
            <small>SHA-256 {b.sha256}</small>
          </div>
        ))}
        <p>
          Logical SQL dumps stay under DEVONE_HOME/backups/mysql. Backup
          consistency uses a single transaction for transactional tables;
          nontransactional tables may change during export. Restore uses the
          database-specific user. No automatic source archive, scheduling or
          deletion.
        </p>
      </details>
    </>
  );
}
