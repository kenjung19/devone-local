import { useState } from "react";
import type { Snapshot, Action, Editor } from "../contracts";
export function DeveloperSettings({
  data,
  busy,
  act,
}: {
  data: Snapshot;
  busy: boolean;
  act: (a: Action) => Promise<void>;
}) {
  const [name, setName] = useState("Custom editor");
  const [exe, setExe] = useState("");
  const [args, setArgs] = useState('["{project}"]');
  const [category, setCategory] = useState<Editor["category"]>("editor");
  const [error, setError] = useState("");
  const [keep, setKeep] = useState(
    data.developer?.backup_preferences.keep_last ?? 7,
  );
  const save = () => {
    try {
      const parsed: unknown = JSON.parse(args);
      if (!Array.isArray(parsed) || parsed.some((a) => typeof a !== "string"))
        throw Error("Arguments must be a JSON array of strings");
      setError("");
      void act({
        type: "save_editor",
        editor: {
          id: `custom-${category}`,
          name,
          executable: exe,
          args: parsed as string[],
          category,
        },
      });
    } catch (e) {
      setError(String(e));
    }
  };
  return (
    <>
      <section className="panel">
        <h2>Editors</h2>
        <p>
          Installed integrations are machine-local. Portable project
          configuration contains no editor paths.
        </p>
        {(data.developer?.editors ?? [])
          .filter((e) => e.category === "editor")
          .map((e) => (
            <div className="runtime-row" key={e.id}>
              <div>
                <strong>{e.name}</strong>
                <small>{e.executable}</small>
              </div>
              <button
                disabled={busy || data.developer?.default_editor === e.id}
                onClick={() =>
                  void act({
                    type: "editor",
                    site_id: null,
                    editor_id: e.id,
                    operation: "default",
                  })
                }
              >
                {data.developer?.default_editor === e.id
                  ? "Default"
                  : "Set default"}
              </button>
            </div>
          ))}
        <details>
          <summary>Custom editor or database client</summary>
          <div className="creation-fields">
            <label>
              Name
              <input value={name} onChange={(e) => setName(e.target.value)} />
            </label>
            <label>
              Executable path
              <input
                value={exe}
                onChange={(e) => setExe(e.target.value)}
                placeholder="Absolute path to an installed .exe"
              />
            </label>
            <label>
              Category
              <select
                value={category}
                onChange={(e) =>
                  setCategory(e.target.value as Editor["category"])
                }
              >
                <option value="editor">Editor</option>
                <option value="database_client">Database client</option>
              </select>
            </label>
            <label>
              Arguments (JSON array)
              <input value={args} onChange={(e) => setArgs(e.target.value)} />
              <small>
                Editors support the {"{project}"} placeholder. Database clients
                open without credentials or connection arguments.
              </small>
            </label>
            {error && <p role="alert">{error}</p>}
            <button disabled={busy} onClick={save}>
              Save machine-local integration
            </button>
          </div>
        </details>
      </section>
      <section className="panel">
        <h2>Templates</h2>
        <p>
          Built-in templates are bundled. Local custom JSON templates are read
          from {data.home}/config/templates. Opening this page never runs their
          commands.
        </p>
        {data.developer?.template_error && (
          <p role="alert">{data.developer.template_error}</p>
        )}
        <p>
          Custom templates require a custom- ID, custom strategy, structured
          managed commands, relative files and exact SHA-256 for remote sources.
        </p>
        <details>
          <summary>Custom template example</summary>
          <pre>
            {JSON.stringify(
              {
                id: "custom-static",
                name: "Team Static",
                category: "Static",
                strategy: "custom",
                runtimes: {},
                tools: [],
                files: { "index.html": "<!doctype html><h1>Team starter</h1>" },
                commands: [],
              },
              null,
              2,
            )}
          </pre>
        </details>
      </section>
      <section className="panel">
        <h2>Backups</h2>
        <p>
          Manual database backups are available on the Databases page. Project
          source remains under your Git workflow. Automatic scheduling and
          deletion are not enabled.
        </p>
        <label>
          Future retention preference
          <input
            type="number"
            min={1}
            max={100}
            value={keep}
            onChange={(e) => setKeep(Number(e.target.value))}
          />
        </label>
        <button
          disabled={busy}
          onClick={() =>
            void act({ type: "backup_preferences", keep_last: keep })
          }
        >
          Save preference
        </button>
      </section>
    </>
  );
}
export function Diagnostics({
  data,
  busy,
  act,
}: {
  data: Snapshot;
  busy: boolean;
  act: (a: Action) => Promise<void>;
}) {
  const [error, setError] = useState("");
  return (
    <section className="panel">
      <h2>Diagnostics</h2>
      <p>
        Check local DNS, CA, runtimes, managed services and ports. Reports
        contain local paths; review before sharing. Nothing is uploaded.
      </p>
      <div className="actions">
        <button
          disabled={busy}
          onClick={() => void act({ type: "diagnostics", export: false })}
        >
          Run Diagnostics
        </button>
        <button
          disabled={!data.developer?.diagnostic_report}
          onClick={() =>
            void navigator.clipboard
              .writeText(data.developer!.diagnostic_report!)
              .catch((e) => setError(String(e)))
          }
        >
          Copy Report
        </button>
        <button
          disabled={busy}
          onClick={() => void act({ type: "diagnostics", export: true })}
        >
          Export JSON report
        </button>
      </div>
      {error && <p role="alert">{error}</p>}
      {data.developer?.diagnostic_report && (
        <pre className="task-log">{data.developer.diagnostic_report}</pre>
      )}
    </section>
  );
}
