import type { Snapshot, Action } from "../contracts";
import {
  editorChoice,
  runtimeName,
  siteProblems,
  siteStatus,
} from "../presentation";
import { Badge } from "./Badge";
import { CaUpgradeNotice } from "./CaUpgradeNotice";
export function SitesOverview({
  data,
  busy,
  act,
  view,
  create,
}: {
  data: Snapshot;
  busy: boolean;
  act: (a: Action) => Promise<void>;
  view: (id: string) => void;
  create: () => void;
}) {
  const editor = editorChoice(data);
  const sites = [...data.sites].sort((a, b) => {
    const av = data.developer?.preferences[a.id],
      bv = data.developer?.preferences[b.id];
    return (
      Number(bv?.favorite ?? false) - Number(av?.favorite ?? false) ||
      Math.max(bv?.opened_at ?? 0, bv?.created_at ?? 0) -
        Math.max(av?.opened_at ?? 0, av?.created_at ?? 0) ||
      a.name.localeCompare(b.name)
    );
  });
  return (
    <>
      <CaUpgradeNotice data={data} busy={busy} act={act} />
      {!data.setup.caddy && (
        <div className="alert">
          Install the Web Server from Runtimes to start sites.
        </div>
      )}
      {!data.dns_ready && (
        <div className="alert">
          Local domains are not configured. Sites cannot open as *.test yet.{" "}
          <button disabled={busy} onClick={() => void act({ type: "dns" })}>
            Set up local domains
          </button>
        </div>
      )}
      {!data.setup.ca_trusted && (
        <div className="alert">
          HTTPS trust is missing.{" "}
          <button
            disabled={busy || !data.setup.caddy}
            onClick={() =>
              void act({ type: data.ca_present ? "trust" : "prepare_ca" })
            }
          >
            {data.ca_present ? "Fix HTTPS" : "Prepare HTTPS"}
          </button>
        </div>
      )}
      <section className="panel">
        <div className="panel-header">
          <h2>Projects</h2>
          <div className="actions">
            <button
              disabled={busy}
              onClick={() => void act({ type: "open_folder", site_id: null })}
            >
              Open www
            </button>
            <button disabled={busy} onClick={() => void act({ type: "scan" })}>
              Refresh
            </button>
          </div>
        </div>
        {!sites.length ? (
          <div className="empty">
            <h3>No projects yet.</h3>
            <p>Copy an existing project into www or create a new one.</p>
            <div className="actions empty-actions">
              <button
                disabled={busy}
                onClick={() => void act({ type: "open_folder", site_id: null })}
              >
                Open www
              </button>
              <button className="primary" onClick={create}>
                New Project
              </button>
            </div>
          </div>
        ) : (
          sites.map((site) => {
            const status = siteStatus(site, data),
              problems = siteProblems(site, data);
            const running =
              site.status === "running" || site.status === "starting";
            return (
              <article className="site-card" key={site.id}>
                <div className="site-card-heading">
                  <div>
                    <button
                      className="site-title"
                      onClick={() => view(site.id)}
                    >
                      {data.developer?.preferences[site.id]?.favorite
                        ? "\u2605 "
                        : ""}
                      {site.hostname}
                    </button>
                    <small>
                      {{
                        plain_php: "PHP",
                        php: "PHP",
                        laravel: "Laravel",
                        next: "Next.js",
                        vite: "Vite",
                        static: "Static HTML",
                        wordpress: "WordPress",
                        node: "Node",
                      }[site.project_type] ?? site.project_type}
                    </small>
                  </div>
                  <Badge value={status} />
                </div>
                <p className="site-runtimes">
                  {Object.entries(site.resolved)
                    .filter(
                      ([kind]) =>
                        kind !== "mysql" ||
                        site.metadata?.requirements.includes("mysql") ||
                        site.metadata?.runtimes.mysql ||
                        data.project_databases.some(
                          (b) => b.site_id === site.id,
                        ),
                    )
                    .map(([kind, version]) => `${runtimeName(kind)} ${version}`)
                    .join(" / ") || "No project runtime required"}
                </p>
                {problems.map((p, i) => (
                  <div className="site-problem" key={i}>
                    <span>{p.message}</span>
                    {p.action && (
                      <button
                        disabled={busy}
                        onClick={() => void act(p.action!)}
                      >
                        {p.label}
                      </button>
                    )}
                  </div>
                ))}
                <div className="actions">
                  <button
                    className="primary"
                    disabled={busy || !site.present}
                    onClick={() =>
                      void act({
                        type: "site_action",
                        site_id: site.id,
                        operation: running ? "stop" : "start",
                      })
                    }
                  >
                    {running ? "Stop" : "Start"}
                  </button>
                  <button
                    disabled={
                      busy ||
                      site.status !== "running" ||
                      !data.dns_ready ||
                      !data.setup.ca_trusted
                    }
                    title="Start this site and finish local domains / HTTPS setup first"
                    onClick={() =>
                      void act({ type: "open_site", site_id: site.id })
                    }
                  >
                    Open Site
                  </button>
                  {editor.selected && (
                    <button
                      disabled={busy || !site.present}
                      onClick={() =>
                        void act({
                          type: "editor",
                          site_id: site.id,
                          editor_id: editor.selected!.id,
                          operation: "open",
                        })
                      }
                    >
                      {editor.label}
                    </button>
                  )}
                  <button
                    disabled={busy || !site.present}
                    onClick={() =>
                      void act({ type: "terminal", site_id: site.id })
                    }
                  >
                    Terminal
                  </button>
                  <button onClick={() => view(site.id)}>Details</button>
                </div>
              </article>
            );
          })
        )}
      </section>
    </>
  );
}
