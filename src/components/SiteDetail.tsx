import type { Site, Snapshot, Action } from "../contracts";
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
  return (
    <>
      <button className="back" onClick={back}>
        ← All sites
      </button>
      <section className="panel">
        <div className="panel-header">
          <div>
            <h2>{site.hostname}</h2>
            <p>{site.project_type === "laravel" ? "Laravel" : "Plain PHP"}</p>
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
          {(["php", "mysql"] as const).map((kind) => (
            <div className="dl-row" key={kind}>
              <dt>{kind.toUpperCase()}</dt>
              <dd>
                <select
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
          <dt>Effective PHP source</dt>
          <dd>
            {site.overrides.php ? "Site override" : "Global default"} ·{" "}
            {site.resolved.php ?? "unconfigured"}
          </dd>
          <dt>Effective MySQL source</dt>
          <dd>
            {data.project_databases.some((b) => b.site_id === site.id)
              ? "Persistent project database binding"
              : site.overrides.mysql
                ? "Site override"
                : "Global default"}{" "}
            · {site.resolved.mysql ?? "unconfigured"}
          </dd>
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
      <Provision site={site} data={data} busy={busy} act={act} />
      {site.issue && <div className="alert error">{site.issue}</div>}
      <p className="footnote">
        Overrides are persisted independently of global defaults. Terminal PATH
        applies only to the new terminal session.
      </p>
    </>
  );
}
