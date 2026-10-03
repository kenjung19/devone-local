import { useState } from "react";
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
        {(err || error) && <p className="error">{err || error}</p>}
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
        onSubmit={(e) => {
          e.preventDefault();
          void act({
            type: "php_config",
            version: runtime.manifest.version,
            config,
          });
        }}
      >
        <h2>PHP {runtime.manifest.version}</h2>
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
