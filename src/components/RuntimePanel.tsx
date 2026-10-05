import type { Snapshot, Action, RuntimeKind, Installation } from "../contracts";
import { runtimeName, versionLabel } from "../presentation";
export function RuntimePanel({
  data,
  busy,
  act,
  importRuntime,
  configure,
}: {
  data: Snapshot;
  busy: boolean;
  act: (a: Action) => Promise<void>;
  importRuntime: (k: RuntimeKind) => void;
  configure: (r: Installation) => void;
}) {
  return (
    <>
      {(["php", "node", "mysql", "caddy"] as const).map((kind) => (
        <section className="panel" key={kind}>
          <div className="panel-header">
            <div>
              <h2>{runtimeName(kind)}</h2>
              <p>
                {kind === "caddy"
                  ? "Local web routing and HTTPS"
                  : "Install the versions your projects need. No automatic upgrades."}
              </p>
            </div>
          </div>
          <h3 className="panel-copy">Installed</h3>
          {data.installed
            .filter((r) => r.manifest.runtime === kind)
            .map((r) => (
              <div className="runtime-row" key={r.id}>
                <div>
                  <strong>{versionLabel(r.manifest)}</strong>
                  <span>
                    {data.defaults[kind] === r.manifest.version
                      ? "Default"
                      : "Installed"}
                  </span>
                </div>
                <div className="actions">
                  {kind === "php" && (
                    <button disabled={busy} onClick={() => configure(r)}>
                      Configure
                    </button>
                  )}
                  <button
                    disabled={busy}
                    onClick={() =>
                      void act({
                        type: "validate",
                        runtime: { kind, version: r.manifest.version },
                      })
                    }
                  >
                    Validate
                  </button>
                  <button
                    disabled={
                      busy || data.defaults[kind] === r.manifest.version
                    }
                    onClick={() =>
                      void act({
                        type: "default",
                        runtime: { kind, version: r.manifest.version },
                      })
                    }
                  >
                    Set default
                  </button>
                  <button
                    disabled={
                      busy || data.defaults[kind] === r.manifest.version
                    }
                    title="Select another default first. Runtimes used by projects cannot be removed."
                    onClick={() => {
                      if (
                        window.confirm(
                          `Remove ${runtimeName(kind)} ${r.manifest.version}? Project files and database data are kept. Runtimes in use cannot be removed.`,
                        )
                      )
                        void act({
                          type: "remove",
                          runtime: { kind, version: r.manifest.version },
                        });
                    }}
                  >
                    Remove
                  </button>
                </div>
                <details className="row-details">
                  <summary>Advanced details</summary>
                  <code>{r.relative_path}</code>
                  <pre>{JSON.stringify(r.manifest, null, 2)}</pre>
                </details>
              </div>
            ))}
          {!data.installed.some((r) => r.manifest.runtime === kind) && (
            <p className="panel-copy">
              No {runtimeName(kind)} installed. Choose a version below.
            </p>
          )}
          <details
            open={!data.installed.some((r) => r.manifest.runtime === kind)}
            className="panel-copy"
          >
            <summary>Available versions</summary>
            {data.available
              .filter(
                (m) =>
                  m.runtime === kind &&
                  m.platform === data.platform &&
                  !data.installed.some((r) => r.id === `${kind}:${m.version}`),
              )
              .map((m) => (
                <div className="runtime-row" key={m.version}>
                  <span>{versionLabel(m)}</span>
                  <button
                    disabled={busy || !m.download || !m.sha256}
                    onClick={() =>
                      void act({
                        type: "install",
                        runtime: { kind, version: m.version },
                      })
                    }
                  >
                    Install
                  </button>
                </div>
              ))}
          </details>
          <details className="panel-copy">
            <summary>Advanced import</summary>
            <button disabled={busy} onClick={() => importRuntime(kind)}>
              Import {runtimeName(kind)}
            </button>
          </details>
        </section>
      ))}
    </>
  );
}
