import { useState } from "react";
import type {
  RuntimeKind,
  Snapshot,
  Action,
  Installation,
  PhpConfig,
} from "../contracts";
import { runtimeBinaries } from "../presentation";
export function ImportDialog({
  kind,
  data,
  busy,
  close,
  act,
}: {
  kind: RuntimeKind;
  data: Snapshot;
  busy: boolean;
  close: () => void;
  act: (a: Action) => Promise<void>;
}) {
  const [version, setVersion] = useState("");
  const [source, setSource] = useState("");
  const [binaries, setBinaries] = useState(
    JSON.stringify(runtimeBinaries(kind), null, 2),
  );
  const [err, setErr] = useState("");
  return (
    <div className="overlay">
      <form
        className="dialog"
        onSubmit={(e) => {
          e.preventDefault();
          try {
            const parsed: unknown = JSON.parse(binaries);
            if (
              !parsed ||
              typeof parsed !== "object" ||
              Array.isArray(parsed) ||
              Object.values(parsed).some((v) => typeof v !== "string")
            )
              throw new Error(
                "Binary roles must be an object of relative paths",
              );
            void act({
              type: "import",
              source,
              manifest: {
                runtime: kind,
                version,
                platform: data.platform,
                binaries: parsed as Record<string, string>,
                metadata: {},
              },
            });
          } catch (e) {
            setErr(String(e));
          }
        }}
      >
        <h2>
          Import {kind === "php" ? "PHP" : kind === "mysql" ? "MySQL" : "Caddy"}{" "}
          runtime
        </h2>
        <p>
          Use a trusted, extracted native distribution. DEVONE copies and
          validates it without changing global PATH.
        </p>
        <label>
          Exact binary version
          <input
            required
            value={version}
            onChange={(e) => setVersion(e.target.value)}
            placeholder="Version reported by --version"
          />
        </label>
        <label>
          Runtime folder
          <input
            required
            value={source}
            onChange={(e) => setSource(e.target.value)}
            placeholder="Absolute path to extracted distribution"
          />
        </label>
        <label>
          Binary roles (relative paths)
          <textarea
            rows={5}
            value={binaries}
            onChange={(e) => setBinaries(e.target.value)}
          />
        </label>
        {err && <p className="error">{err}</p>}
        <div className="actions">
          <button type="button" onClick={close}>
            Close
          </button>
          <button className="primary" disabled={busy}>
            {busy ? "Validating…" : "Import & validate"}
          </button>
        </div>
        <small>
          First imported runtime in each category becomes its default.
        </small>
      </form>
    </div>
  );
}
export function PhpDialog({
  runtime,
  busy,
  close,
  act,
}: {
  runtime: Installation;
  busy: boolean;
  close: () => void;
  act: (a: Action) => Promise<void>;
}) {
  const [text, setText] = useState("");
  const [err, setErr] = useState("");
  // Require explicit configuration content so opening this dialog cannot erase existing settings.
  return (
    <div className="overlay">
      <form
        className="dialog"
        onSubmit={(e) => {
          e.preventDefault();
          try {
            const c: unknown = JSON.parse(text);
            if (
              !c ||
              typeof c !== "object" ||
              !("directives" in c) ||
              !("extensions" in c)
            )
              throw new Error("Provide directives and extensions");
            void act({
              type: "php_config",
              version: runtime.manifest.version,
              config: c as PhpConfig,
            });
          } catch (e) {
            setErr(String(e));
          }
        }}
      >
        <h2>PHP {runtime.manifest.version}</h2>
        <p>
          Save a complete configuration to replace DEVONE-managed overrides. A
          running pool restarts after saving. The distribution’s php.ini is
          preserved.
        </p>
        <label>
          Directives and enabled extensions
          <textarea
            required
            rows={10}
            value={text}
            placeholder={
              '{\n  "directives": { "memory_limit": "256M" },\n  "extensions": ["curl", "mbstring", "pdo_mysql"]\n}'
            }
            onChange={(e) => setText(e.target.value)}
          />
        </label>
        {err && <p className="error">{err}</p>}
        <div className="actions">
          <button type="button" onClick={close}>
            Close
          </button>
          <button
            type="button"
            onClick={() =>
              void act({ type: "open_ini", version: runtime.manifest.version })
            }
          >
            Open generated ini
          </button>
          <button className="primary" disabled={busy}>
            Save configuration
          </button>
        </div>
      </form>
    </div>
  );
}
