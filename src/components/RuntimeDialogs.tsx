import { useState, useEffect } from "react";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { bridge, desktop } from "../bridge";
import type {
  RuntimeKind,
  Snapshot,
  Action,
  Installation,
  PhpConfig,
} from "../contracts";

export function ImportDialog({
  kind,
  data,
  busy,
  error,
  close,
  act,
}: {
  kind: RuntimeKind;
  data: Snapshot;
  busy: boolean;
  error: string;
  close: () => void;
  act: (a: Action) => Promise<void>;
}) {
  const [version, setVersion] = useState("");
  const [source, setSource] = useState("");
  const [binaries, setBinaries] = useState(
    JSON.stringify(data.binary_roles[kind], null, 2),
  );
  const [err, setErr] = useState("");
  const [inspecting, setInspecting] = useState(false);
  useEffect(() => {
    if (!desktop) return;
    let disposed = false;
    let unlisten: (() => void) | undefined;
    void getCurrentWebview()
      .onDragDropEvent((event) => {
        if (event.payload.type === "drop" && event.payload.paths[0]) {
          setSource(event.payload.paths[0]);
          setVersion("");
        }
      })
      .then((remove) => {
        if (disposed) remove();
        else unlisten = remove;
      })
      .catch((e) => setErr(String(e)));
    return () => {
      disposed = true;
      unlisten?.();
    };
  }, []);
  return (
    <div className="overlay">
      <form
        className="dialog"
        role="dialog"
        aria-modal="true"
        aria-labelledby="runtime-dialog-title"
        onKeyDown={(e) => {
          if (e.key === "Escape" && !busy) close();
        }}
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
        <h2 id="runtime-dialog-title">
          Import{" "}
          {kind === "php"
            ? "PHP"
            : kind === "mysql"
              ? "MySQL"
              : kind === "node"
                ? "Node"
                : "Web Server"}{" "}
          runtime
        </h2>
        <p>
          Use a trusted, extracted native distribution. DEVONE copies and
          validates it without changing global PATH. Drop its folder here or
          paste the folder path, then detect the version.
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
        <button
          type="button"
          disabled={busy || inspecting || !source}
          onClick={() => {
            setInspecting(true);
            setErr("");
            void bridge
              .inspectImport(kind, source)
              .then((m) => setVersion(m.version))
              .catch((e) => setErr(String(e)))
              .finally(() => setInspecting(false));
          }}
        >
          {inspecting ? "Detecting…" : "Detect version"}
        </button>
        <details>
          <summary>Advanced import settings</summary>
          <label>
            Binary roles (relative paths)
            <textarea
              rows={5}
              value={binaries}
              onChange={(e) => setBinaries(e.target.value)}
            />
          </label>
        </details>
        {(err || error) && <p className="error">{err || error}</p>}
        <div className="actions">
          <button type="button" onClick={close}>
            Close
          </button>
          <button className="primary" disabled={busy || inspecting}>
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
  data,
  busy,
  error,
  close,
  act,
}: {
  runtime: Installation;
  data: Snapshot;
  busy: boolean;
  error: string;
  close: () => void;
  act: (a: Action) => Promise<void>;
}) {
  const settings = data.php_settings[runtime.manifest.version];
  const [config, setConfig] = useState<PhpConfig>(
    settings?.config ?? { directives: {}, extensions: [] },
  );
  const directives = [
    "memory_limit",
    "upload_max_filesize",
    "post_max_size",
    "max_execution_time",
    "date.timezone",
    "display_errors",
    "error_reporting",
  ];
  return (
    <div className="overlay">
      <form
        className="dialog"
        role="dialog"
        aria-modal="true"
        aria-labelledby="runtime-dialog-title"
        onKeyDown={(e) => {
          if (e.key === "Escape" && !busy) close();
        }}
        onSubmit={(e) => {
          e.preventDefault();
          void act({
            type: "php_config",
            version: runtime.manifest.version,
            config,
          });
        }}
      >
        <h2 id="runtime-dialog-title">PHP {runtime.manifest.version}</h2>
        {error && (
          <p className="error" role="alert">
            {error}
          </p>
        )}
        <p>
          ตรวจ php -v, --ini, -m ก่อนบันทึก และ restart เฉพาะ pool เวอร์ชันนี้
          Original php.ini คงเดิม
        </p>
        {directives.map((key) => (
          <label key={key}>
            {key}
            <input
              value={config.directives[key] ?? ""}
              placeholder="ใช้ค่าเดิมของ PHP"
              onChange={(e) => {
                const values = { ...config.directives };
                if (e.target.value) values[key] = e.target.value;
                else delete values[key];
                setConfig({ ...config, directives: values });
              }}
            />
          </label>
        ))}
        <fieldset>
          <legend>Extensions ที่พบใน distribution นี้</legend>
          {settings?.available_extensions.length ? (
            settings.available_extensions.map((file) => {
              const short = file
                .replace(/^php_/, "")
                .replace(/\.(dll|so)$/, "");
              const checked =
                config.extensions.includes(file) ||
                config.extensions.includes(short);
              return (
                <label className="extension-choice" key={file}>
                  <input
                    type="checkbox"
                    checked={checked}
                    onChange={(e) =>
                      setConfig({
                        ...config,
                        extensions: e.target.checked
                          ? [...config.extensions, file]
                          : config.extensions.filter(
                              (v) => v !== file && v !== short,
                            ),
                      })
                    }
                  />
                  {file}
                </label>
              );
            })
          ) : (
            <p>ไม่พบ extension แยกในโฟลเดอร์ ext</p>
          )}
        </fieldset>
        <div className="actions">
          <button type="button" onClick={close}>
            Close
          </button>
          <button
            type="button"
            disabled={busy}
            onClick={() =>
              void act({ type: "open_ini", version: runtime.manifest.version })
            }
          >
            Open managed ini
          </button>
          <button className="primary" disabled={busy}>
            Validate & save
          </button>
        </div>
      </form>
    </div>
  );
}
