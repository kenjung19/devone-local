import { useEffect, useMemo, useState } from "react";
import type {
  Action,
  EnvironmentProfile,
  RuntimeKind,
  Site,
  Snapshot,
} from "../contracts";

const webProviders = ["caddy", "apache", "nginx"] as const;
const runtimeLabels: Record<string, string> = {
  web_server: "Web Server",
  php: "PHP",
  mysql: "MySQL",
  node: "Node.js",
};

function friendlyProvider(provider: string) {
  return provider === "nginx"
    ? "Nginx"
    : provider === "apache"
      ? "Apache"
      : provider === "caddy"
        ? "Caddy"
        : provider;
}

function profileTarget(
  profile: EnvironmentProfile,
  data: Snapshot,
  kind: string,
) {
  const binding = profile.bindings[kind];
  if (!binding || binding.version === "none") return null;
  const version = binding.version ?? data.defaults[binding.provider] ?? null;
  if (!version) return null;
  return kind === "web_server" ? `${binding.provider}:${version}` : version;
}

export function SiteEnvironmentPanel({
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
  const appliedProfileId =
    site.last_applied_profile_id ?? site.runtime_sources?.__profile ?? "";
  const [profileId, setProfileId] = useState(appliedProfileId);
  useEffect(() => setProfileId(appliedProfileId), [appliedProfileId]);

  const database = data.project_databases.find(
    (binding) => binding.site_id === site.id,
  );
  const local = site.local_overrides ?? site.overrides;
  const runtimeKinds = (["php", "mysql", "node"] as const).filter((kind) => {
    if (kind === "mysql" && database) return true;
    return (
      kind in site.resolved ||
      kind in local ||
      site.metadata?.requirements.includes(kind) ||
      site.metadata?.runtimes[kind] !== undefined ||
      (kind === "php" &&
        ["php", "plain_php", "laravel", "wordpress"].includes(
          site.project_type,
        )) ||
      (kind === "node" &&
        (site.project_type === "node" ||
          site.metadata?.route === "node_proxy"))
    );
  });
  const toolKinds = (["composer", "pnpm"] as const).filter((tool) =>
    tool === "composer"
      ? site.project_type === "laravel" ||
        !!site.metadata?.composer_manifest ||
        !!site.tool_overrides?.composer
      : runtimeKinds.includes("node") ||
        site.metadata?.package_manager === "pnpm" ||
        !!site.tool_overrides?.pnpm,
  );

  const runtimeChoices = (kind: RuntimeKind) => [
    ...new Map(
      [...data.installed.map((item) => item.manifest), ...data.available]
        .filter((item) => item.runtime === kind)
        .map((item) => [item.version, item]),
    ).values(),
  ];
  const webChoices = [
    ...new Map(
      [...data.installed.map((item) => item.manifest), ...data.available]
        .filter((item) =>
          webProviders.includes(item.runtime as (typeof webProviders)[number]),
        )
        .map((item) => [`${item.runtime}:${item.version}`, item]),
    ).values(),
  ];
  const toolChoices = (tool: "composer" | "pnpm") => [
    ...new Map(
      [...(data.tools ?? []), ...(data.available_tools ?? [])]
        .filter((item) => item.id === tool)
        .map((item) => [item.version, item]),
    ).values(),
  ];

  const appliedProfile = (data.profiles ?? []).find(
    (profile) => profile.id === appliedProfileId,
  );
  const customized = useMemo(() => {
    if (!appliedProfile) return false;
    const runtimeChanged = Object.keys(appliedProfile.bindings).some(
      (kind) =>
        (local[kind] ?? null) !==
        profileTarget(appliedProfile, data, kind),
    );
    const toolChanged = Object.entries(appliedProfile.tools).some(
      ([tool, version]) =>
        (site.tool_overrides?.[tool] ?? null) !==
        (version === "none"
          ? null
          : version ?? data.tool_defaults?.[tool] ?? null),
    );
    return runtimeChanged || toolChanged;
  }, [appliedProfile, data, local, site.tool_overrides]);

  const missingRuntimes = [
    site.resolved.web_server,
    ...runtimeKinds.map((kind) => site.resolved[kind]),
  ].flatMap((binding, index) => {
    if (!binding) return [];
    const [kind, version] =
      index === 0 ? binding.split(":", 2) : [runtimeKinds[index - 1], binding];
    return data.installed.some(
      (item) =>
        item.manifest.runtime === kind && item.manifest.version === version,
    )
      ? []
      : [{ kind: kind as RuntimeKind, version }];
  });
  const missingTools = toolKinds.flatMap((tool) => {
    const version = site.resolved_tools?.[tool];
    return version &&
      !(data.tools ?? []).some(
        (item) => item.id === tool && item.version === version,
      )
      ? [{ tool, version }]
      : [];
  });
  const stack = [
    site.resolved.web_server
      ? site.resolved.web_server
          .split(":")
          .map((part, index) => (index === 0 ? friendlyProvider(part) : part))
          .join(" ")
      : null,
    ...runtimeKinds.map((kind) =>
      site.resolved[kind]
        ? `${runtimeLabels[kind]} ${site.resolved[kind]}`
        : null,
    ),
    ...toolKinds.map((tool) =>
      site.resolved_tools?.[tool]
        ? `${tool} ${site.resolved_tools[tool]}`
        : null,
    ),
  ].filter(Boolean);

  return (
    <section className="panel">
      <div className="panel-header">
        <div>
          <h2>Environment</h2>
          <p className="panel-copy">
            {stack.join(" · ") || "No runtime selected"}
          </p>
        </div>
        {appliedProfile && (
          <span>
            Based on {appliedProfile.name} ·{" "}
            {customized ? "Customized" : "Last applied"}
          </span>
        )}
      </div>
      <p className="panel-copy">
        Applying a profile copies values into this site. Later profile edits do
        not change this site automatically.
      </p>
      <div className="creation-fields">
        <label>
          Profile
          <span className="actions">
            <select
              aria-label="Environment Profile"
              value={profileId}
              disabled={busy}
              onChange={(event) => setProfileId(event.target.value)}
            >
              <option value="">None</option>
              {(data.profiles ?? []).map((profile) => (
                <option key={profile.id} value={profile.id}>
                  {profile.name}
                </option>
              ))}
            </select>
            <button
              disabled={busy || !profileId}
              onClick={() =>
                void act({
                  type: "apply_profile",
                  site_id: site.id,
                  profile_id: profileId,
                })
              }
            >
              {profileId === appliedProfileId ? "Reapply" : "Apply"}
            </button>
          </span>
        </label>

        <h3>Web</h3>
        <label>
          Web Server
          <select
            aria-label="Web Server version"
            value={site.resolved.web_server ?? ""}
            disabled={busy}
            onChange={(event) =>
              void act({
                type: "set_web_server",
                site_id: site.id,
                binding: event.target.value,
              })
            }
          >
            <option value="">Select a Web Server</option>
            {webChoices.map((item) => {
              const value = `${item.runtime}:${item.version}`;
              const installed = data.installed.some(
                (runtime) =>
                  runtime.manifest.runtime === item.runtime &&
                  runtime.manifest.version === item.version,
              );
              return (
                <option key={value} value={value}>
                  {friendlyProvider(item.runtime)} {item.version}
                  {installed ? "" : " · Not installed"}
                </option>
              );
            })}
          </select>
        </label>

        {runtimeKinds.some((kind) => kind === "php" || kind === "node") && (
          <h3>Backend</h3>
        )}
        {runtimeKinds.map((kind) => (
          <div key={kind}>
            {kind === "mysql" && <h3>Database</h3>}
            <label>
              {runtimeLabels[kind]}
              <select
                aria-label={`${runtimeLabels[kind]} version`}
                value={local[kind] ?? ""}
                disabled={busy || (kind === "mysql" && !!database)}
                onChange={(event) =>
                  void act({
                    type: "override",
                    site_id: site.id,
                    kind,
                    version: event.target.value || null,
                  })
                }
              >
                <option value="">
                  {site.runtime_sources?.[kind] ?? "Detected/default"} (
                  {site.resolved[kind] ?? "not configured"})
                </option>
                {runtimeChoices(kind).map((item) => {
                  const installed = data.installed.some(
                    (runtime) =>
                      runtime.manifest.runtime === kind &&
                      runtime.manifest.version === item.version,
                  );
                  return (
                    <option key={item.version} value={item.version}>
                      {item.version}
                      {installed ? "" : " · Not installed"}
                    </option>
                  );
                })}
              </select>
            </label>
            {kind === "mysql" && database && (
              <p className="panel-copy">
                MySQL runtime: {database.runtime_id.replace("mysql:", "")} ·
                Database: {database.database_name}. Automatic migration is not
                supported; changing its runtime requires an explicit database
                migration workflow.
              </p>
            )}
          </div>
        ))}

        {toolKinds.length > 0 && <h3>Tools</h3>}
        {toolKinds.map((tool) => (
          <label key={tool}>
            {tool === "composer" ? "Composer" : "pnpm"}
            <select
              aria-label={`${tool === "composer" ? "Composer" : "pnpm"} version`}
              value={site.tool_overrides?.[tool] ?? ""}
              disabled={busy}
              onChange={(event) =>
                void act({
                  type: "set_tool_override",
                  site_id: site.id,
                  tool_id: tool,
                  version: event.target.value || null,
                })
              }
            >
              <option value="">
                {site.tool_sources?.[tool] ?? "Detected/default"} (
                {site.resolved_tools?.[tool] ?? "not configured"})
              </option>
              {toolChoices(tool).map((item) => {
                const installed = (data.tools ?? []).some(
                  (installed) =>
                    installed.id === tool &&
                    installed.version === item.version,
                );
                return (
                  <option key={item.version} value={item.version}>
                    {item.version}
                    {installed ? "" : " · Not installed"}
                  </option>
                );
              })}
            </select>
          </label>
        ))}

        {missingRuntimes.map((missing) => (
          <div role="alert" key={`${missing.kind}:${missing.version}`}>
            {friendlyProvider(missing.kind)} {missing.version} is not installed.{" "}
            <button
              disabled={
                busy ||
                !data.available.some(
                  (item) =>
                    item.runtime === missing.kind &&
                    item.version === missing.version,
                )
              }
              onClick={() => void act({ type: "install", runtime: missing })}
            >
              Install
            </button>
          </div>
        ))}
        {missingTools.map((missing) => (
          <div role="alert" key={`${missing.tool}:${missing.version}`}>
            {missing.tool} {missing.version} is not installed.{" "}
            <button
              disabled={
                busy ||
                !(data.available_tools ?? []).some(
                  (item) =>
                    item.id === missing.tool &&
                    item.version === missing.version,
                )
              }
              onClick={() =>
                void act({
                  type: "install_tool",
                  id: missing.tool,
                  version: missing.version,
                  node:
                    (missing.tool === "composer"
                      ? site.resolved.php
                      : site.resolved.node) ?? null,
                })
              }
            >
              Install
            </button>
          </div>
        ))}
      </div>
    </section>
  );
}
