import type { Action, RuntimeKind, Site, Snapshot } from "../contracts";

export function SiteEnvironment({
  site,
  data,
  busy,
  act,
}: {
  site: Site;
  data: Snapshot;
  busy: boolean;
  act: (action: Action) => Promise<boolean>;
}) {
  const profile = site.runtime_sources?.__profile ?? site.last_applied_profile_id ?? "";
  const web = site.resolved.web_server ?? data.defaults.web_server ?? "";
  const choices = [
    ...new Map(
      [...data.installed.map((item) => item.manifest), ...data.available]
        .filter((item) => ["caddy", "apache", "nginx"].includes(item.runtime))
        .map((item) => [`${item.runtime}:${item.version}`, item]),
    ).values(),
  ];
  const missing = Object.entries(site.resolved).flatMap(([kind, version]) => {
    const runtime = kind === "web_server" ? version.split(":") : [kind, version];
    return data.installed.some(
      (item) =>
        item.manifest.runtime === runtime[0] && item.manifest.version === runtime[1],
    )
      ? []
      : [{ kind: runtime[0] as RuntimeKind, version: runtime[1] }];
  });
  return (
    <section className="panel">
      <h2>Environment</h2>
      <p className="panel-copy">
        These bindings belong to this site. Applying a profile copies values here;
        later profile edits do not change this site.
      </p>
      <div className="creation-fields">
        <label>
          Profile
          <span className="actions">
            <select id={`profile-${site.id}`} defaultValue={profile}>
              <option value="">Choose a profile</option>
              {(data.profiles ?? []).map((item) => (
                <option key={item.id} value={item.id}>{item.name}</option>
              ))}
            </select>
            <button
              disabled={busy}
              onClick={() => {
                const element = document.getElementById(`profile-${site.id}`) as HTMLSelectElement;
                if (element.value)
                  void act({ type: "apply_profile", site_id: site.id, profile_id: element.value });
              }}
            >
              Apply
            </button>
          </span>
        </label>
        <label>
          Web Server
          <select
            value={web}
            disabled={busy}
            onChange={(event) =>
              void act({ type: "set_web_server", site_id: site.id, binding: event.target.value })
            }
          >
            <option value="">Select a Web Server</option>
            {choices.map((item) => {
              const value = `${item.runtime}:${item.version}`;
              const installed = data.installed.some(
                (runtime) => runtime.manifest.runtime === item.runtime && runtime.manifest.version === item.version,
              );
              return <option key={value} value={value}>{item.runtime} {item.version}{installed ? "" : " · Not installed"}</option>;
            })}
          </select>
        </label>
        {missing.map((runtime) => (
          <div role="alert" key={`${runtime.kind}:${runtime.version}`}>
            {runtime.kind} {runtime.version} is not installed.{" "}
            <button
              disabled={busy || !data.available.some((item) => item.runtime === runtime.kind && item.version === runtime.version)}
              onClick={() => void act({ type: "install", runtime })}
            >
              Install
            </button>
          </div>
        ))}
      </div>
    </section>
  );
}
