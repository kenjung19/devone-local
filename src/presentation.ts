import type { Site, Snapshot, Action, Manifest, Template } from "./contracts";
export const runtimeName = (kind: string) =>
  ({ php: "PHP", node: "Node", mysql: "MySQL", caddy: "Web Server" })[kind] ??
  kind;
export function versionLabel(m: Manifest) {
  return `${runtimeName(m.runtime)} ${m.version}${m.metadata.channel ? ` ${m.metadata.channel}` : ""}`;
}
export function userError(error: string) {
  if (/port|address.*in use|10048/i.test(error))
    return "A required port is unavailable. Stop the application using it, then retry.";
  if (/not installed|missing runtime|default.*missing|no default/i.test(error))
    return "A required runtime or tool is missing. Install or select it, then retry.";
  if (/dependencies|node_modules|vendor/i.test(error))
    return "Project dependencies need attention. Check the installation and retry.";
  if (/DNS|NRPT|resolver/i.test(error))
    return "Local domains are unavailable. Set up local domains in Settings and retry.";
  if (/certificate|trust|CA\b/i.test(error))
    return "HTTPS needs attention. Check HTTPS in Settings and retry.";
  if (/network|download|request|connect|timed out/i.test(error))
    return "The operation could not finish. Check the connection or service, then retry.";
  if (
    /confirm|protected|referenced|cannot.*remove|already exists|invalid|name/i.test(
      error,
    )
  )
    return "This action could not be completed safely. Review the selection and details below.";
  return "The operation could not be completed. Review the details below and retry.";
}
export function editorChoice(data: Snapshot) {
  const editors =
    data.developer?.editors.filter((e) => e.category === "editor") ?? [];
  const selected =
    editors.find((e) => e.id === data.developer?.default_editor) ??
    (editors.length === 1 ? editors[0] : undefined);
  return {
    editors,
    selected,
    label:
      editors.length === 1
        ? `Open in ${editors[0].name}`
        : "Open in Default Editor",
  };
}
export function siteProblems(
  site: Site,
  data: Snapshot,
): { message: string; label?: string; action?: Action }[] {
  const result: { message: string; label?: string; action?: Action }[] = [];
  if (!site.present)
    return [
      {
        message:
          "Project folder is missing. Restore it to www to use this site.",
      },
    ];
  if (site.issue || site.metadata?.error)
    result.push({
      message:
        "Project configuration needs attention. Open site details to review it.",
    });
  const kinds = new Set([
    ...(site.metadata?.requirements ?? []),
    ...Object.keys(site.resolved).filter((k) => k !== "mysql"),
  ]);
  if (
    site.metadata?.runtimes.mysql ||
    data.project_databases.some((b) => b.site_id === site.id)
  )
    kinds.add("mysql");
  for (const kind of kinds) {
    const version = site.resolved[kind];
    if (!version) {
      result.push({
        message: `Choose a ${runtimeName(kind)} version in site details or Runtimes.`,
      });
      continue;
    }
    if (
      !data.installed.some(
        (r) => r.manifest.runtime === kind && r.manifest.version === version,
      )
    ) {
      const available = data.available.some(
        (m) =>
          m.runtime === kind &&
          m.version === version &&
          m.platform === data.platform &&
          m.download &&
          m.sha256,
      );
      result.push({
        message: `${runtimeName(kind)} ${version} is not installed`,
        label: available
          ? `Install ${runtimeName(kind)} ${version}`
          : undefined,
        action: available
          ? {
              type: "install",
              runtime: { kind: kind as "php" | "node" | "mysql", version },
            }
          : undefined,
      });
    } else if (
      kind === "mysql" &&
      !data.databases.some(
        (d) => d.runtime_id === `mysql:${version}` && d.initialized,
      )
    )
      result.push({
        message: `MySQL ${version} is not initialized`,
        label: "Initialize",
        action: {
          type: "database",
          runtime: { kind: "mysql", version },
          operation: "initialize",
        },
      });
  }
  for (const [needed, manager] of [
    [
      site.metadata?.requirements.includes("node") &&
        !site.metadata?.node_dependencies,
      "pnpm",
    ],
    [
      site.metadata?.composer_manifest && !site.metadata?.composer_dependencies,
      "composer",
    ],
  ] as const) {
    if (!needed) continue;
    if (
      manager === "pnpm" &&
      site.metadata?.package_manager &&
      site.metadata.package_manager !== "pnpm"
    ) {
      result.push({
        message:
          "Node dependencies are missing. Install them in the project terminal.",
      });
      continue;
    }
    const version =
      manager === "pnpm"
        ? (site.metadata?.package_manager_version ?? data.tool_defaults?.pnpm)
        : data.tool_defaults?.composer;
    if (!version) {
      result.push({
        message: `Choose a ${manager} version in Tools before installing dependencies.`,
      });
      continue;
    }
    const installed = data.tools?.some(
      (t) => t.id === manager && t.version === version,
    );
    const runtime = site.resolved[manager === "pnpm" ? "node" : "php"];
    result.push(
      installed
        ? {
            message: `${manager === "pnpm" ? "Node" : "PHP"} dependencies missing`,
            label: "Install Dependencies",
            action: { type: "install_dependencies", site_id: site.id, manager },
          }
        : {
            message: `${manager} ${version} is not installed`,
            label: `Install ${manager} ${version}`,
            action:
              runtime &&
              data.available_tools?.some(
                (t) => t.id === manager && t.version === version,
              )
                ? { type: "install_tool", id: manager, version, node: runtime }
                : undefined,
          },
    );
  }
  return result;
}
export function siteStatus(site: Site, data: Snapshot) {
  if (!site.present) return "Missing folder";
  if (["failed", "conflict", "unhealthy"].includes(site.status)) return "Error";
  if (site.status === "starting") return "Starting";
  if (site.status === "running") return "Running";
  if (siteProblems(site, data).length) return "Needs setup";
  return site.processes?.some((p) => p.enabled) ||
    data.services.some((s) => s.key === `php:${site.resolved.php}`)
    ? "Stopped"
    : "Ready";
}
export function bindingLabel(
  site: Pick<Site, "overrides" | "resolved" | "runtime_sources">,
  kind: string,
): string {
  const version = site.resolved[kind];
  if (!version) return "Not configured";
  return (
    version +
    (site.runtime_sources?.[kind]
      ? ` · ${site.runtime_sources[kind]}`
      : site.overrides[kind]
        ? " · override"
        : " · default")
  );
}
export function availableSiteCount(sites: Pick<Site, "present">[]): number {
  return sites.filter((s) => s.present).length;
}
export function runtimeBinaries(kind: string): Record<string, string> {
  if (kind === "node") return { cli: "node.exe" };
  if (kind === "php") return { cli: "php.exe", fastcgi: "php-cgi.exe" };
  if (kind === "mysql")
    return { server: "bin/mysqld.exe", admin: "bin/mysqladmin.exe" };
  return { server: "caddy.exe" };
}

export function creationStage(stage: string) {
  if (/install|composer/i.test(stage)) return "Installing dependencies";
  if (/database/i.test(stage)) return "Creating database";
  if (/configur/i.test(stage)) return "Configuring project";
  if (/final/i.test(stage)) return "Finalizing";
  if (/complete/i.test(stage)) return "Completed";
  if (/interrupt/i.test(stage)) return "Interrupted";
  return "Creating project";
}
export function initialVersions(data: Snapshot, template?: Template) {
  const result = { ...data.defaults };
  for (const kind of ["php", "mysql", "node", "caddy"]) {
    const minimum = template?.runtimes[kind];
    const compatible = (v: string) =>
      !minimum || v.localeCompare(minimum, undefined, { numeric: true }) >= 0;
    if (result[kind] && compatible(result[kind])) continue;
    const installed = data.installed
      .filter(
        (r) => r.manifest.runtime === kind && compatible(r.manifest.version),
      )
      .map((r) => r.manifest);
    const available = data.available.filter(
      (m) =>
        m.runtime === kind &&
        m.platform === data.platform &&
        compatible(m.version),
    );
    const ordered = (installed.length ? installed : available).sort(
      (a, b) =>
        Number(b.metadata.channel === "LTS") -
          Number(a.metadata.channel === "LTS") ||
        b.version.localeCompare(a.version, undefined, { numeric: true }),
    );
    result[kind] = ordered[0]?.version ?? "";
  }
  return result;
}
