import { useState } from "react";
import type { Snapshot, Action } from "../contracts";
export function ProductSettings({
  data,
  busy,
  act,
}: {
  data: Snapshot;
  busy: boolean;
  act: (a: Action) => Promise<void>;
}) {
  const [confirmCa, setConfirmCa] = useState(false);
  return (
    <>
      <section className="panel">
        <div className="panel-header">
          <h2>Desktop & startup</h2>
        </div>
        <div className="settings-row">
          <label>
            <input
              type="checkbox"
              checked={data.startup.enabled}
              disabled={
                busy || !data.startup.supported || data.startup.conflict
              }
              onChange={(e) =>
                void act({ type: "startup", enabled: e.target.checked })
              }
            />{" "}
            Start DEVONE Local with Windows
          </label>
        </div>
        {data.startup.conflict && (
          <p role="alert">
            A startup entry belongs to a different installation or Home. It will
            not be overwritten.
          </p>
        )}
        <div className="settings-row">
          <label>
            <input
              type="checkbox"
              checked={data.environment_autostart}
              disabled={busy}
              onChange={(e) =>
                void act({
                  type: "environment_autostart",
                  enabled: e.target.checked,
                })
              }
            />{" "}
            Start environment automatically when DEVONE starts
          </label>
        </div>
        <p className="panel-copy">
          Closing the window keeps DEVONE and the www watcher running in the
          tray. Choose Quit DEVONE Local to stop owned services. Start/Stop All
          does not change these preferences.
        </p>
        <div className="panel-buttons">
          <button
            disabled={busy}
            onClick={() => void act({ type: "open_folder", site_id: null })}
          >
            Open www
          </button>
        </div>
      </section>
      <section className="panel">
        <div className="panel-header">
          <h2>Wildcard .test DNS</h2>
        </div>
        <dl>
          <dt>Policy</dt>
          <dd>{data.setup.dns_policy ? "Installed" : "Not installed"}</dd>
          <dt>Resolver</dt>
          <dd>{data.setup.dns_server ? "Running" : "Stopped"}</dd>
          <dt>System lookup</dt>
          <dd>
            {data.setup.dns_policy &&
            data.setup.dns_server &&
            data.setup.dns_system
              ? "Ready"
              : "Not ready"}
          </dd>
        </dl>
        <p className="panel-copy">
          .test names resolve while DEVONE is running, including when it is in
          the tray. Quit stops the resolver; the owned Windows policy remains
          installed. Other domains keep their existing DNS configuration.
        </p>
        <div className="panel-buttons">
          <button disabled={busy} onClick={() => void act({ type: "dns" })}>
            Setup / Retry DNS
          </button>
          <button
            disabled={busy || !data.setup.dns_policy}
            onClick={() => void act({ type: "remove_dns" })}
          >
            Remove DEVONE DNS integration
          </button>
        </div>
      </section>
      <section className="panel">
        <div className="panel-header">
          <h2>Local CA</h2>
        </div>
        <dl>
          <dt>CA</dt>
          <dd>{data.setup.ca_present ? "Created" : "Not created"}</dd>
          <dt>Current Windows user</dt>
          <dd>{data.setup.ca_trusted ? "Trusted" : "Not trusted"}</dd>
        </dl>
        <div className="panel-buttons">
          <button
            disabled={busy || !data.setup.caddy}
            onClick={() => void act({ type: "prepare_ca" })}
          >
            Prepare CA
          </button>
          <button
            disabled={busy || !data.setup.ca_present}
            onClick={() => void act({ type: "trust" })}
          >
            Install trust
          </button>
          <button
            disabled={busy || !data.setup.ca_trusted}
            onClick={() => void act({ type: "remove_trust" })}
          >
            Remove trust
          </button>
          <button
            disabled={busy || !data.setup.caddy}
            onClick={() => setConfirmCa(true)}
          >
            Recreate CA…
          </button>
        </div>
      </section>
      {confirmCa && (
        <div className="overlay">
          <section
            className="dialog"
            role="dialog"
            aria-labelledby="recreate-ca-title"
          >
            <h2 id="recreate-ca-title">Recreate DEVONE Local CA?</h2>
            <p>
              Existing generated HTTPS certificates will be regenerated. The old
              CA is preserved in backups. Trust for the old owned CA is removed;
              install trust for the new CA after recreation. Unrelated
              certificates are preserved.
            </p>
            <div className="actions">
              <button onClick={() => setConfirmCa(false)}>Cancel</button>
              <button
                disabled={busy}
                onClick={() => {
                  setConfirmCa(false);
                  void act({ type: "recreate_ca", confirmed: true });
                }}
              >
                Recreate CA
              </button>
            </div>
          </section>
        </div>
      )}
    </>
  );
}
