import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import type { Site, Snapshot } from "../contracts";
import { Profiles } from "./Profiles";
import { SiteEnvironmentPanel } from "./SiteEnvironmentPanel";

const act = async () => true;

function site(projectType: string, requirements: string[] = []): Site {
  return {
    id: "site-1",
    name: "demo",
    hostname: "demo.test",
    project_path: "D:/home/www/demo",
    project_type: projectType,
    document_root: "D:/home/www/demo",
    present: true,
    issue: null,
    overrides: {},
    resolved: {},
    status: "stopped",
    https: "ready",
    discovered_at: 1,
    updated_at: 1,
    metadata: {
      framework: projectType,
      requirements,
      runtimes: {},
      package_manager: requirements.includes("node") ? "pnpm" : null,
      package_manager_version: requirements.includes("node") ? "10.30.1" : null,
      dev_script: null,
      build_script: null,
      route: projectType === "static" ? "static" : "php_fastcgi",
      node_dependencies: false,
      composer_dependencies: projectType === "laravel",
      error: null,
    },
  };
}

function snapshot(selected: Site): Snapshot {
  const manifests = [
    { runtime: "caddy", version: "2.10.2", platform: "windows-x64", binaries: { server: "caddy.exe" }, metadata: {} },
    { runtime: "php", version: "8.5.1", platform: "windows-x64", binaries: { cli: "php.exe", fastcgi: "php-cgi.exe" }, metadata: {} },
    { runtime: "mysql", version: "9.7.2", platform: "windows-x64", binaries: { server: "mysqld.exe" }, metadata: {} },
    { runtime: "node", version: "24.21.0", platform: "windows-x64", binaries: { cli: "node.exe" }, metadata: {} },
  ];
  return {
    home: "D:/home",
    platform: "windows-x64",
    sites: [selected],
    installed: manifests.map((manifest) => ({
      id: `${manifest.runtime}:${manifest.version}`,
      manifest,
      relative_path: `runtimes/${manifest.runtime}/${manifest.version}`,
      installed_at: 1,
    })),
    available: [...manifests, { ...manifests[1], version: "8.6.0" }],
    defaults: { web_server: "caddy:2.10.2", caddy: "2.10.2", php: "8.5.1", mysql: "9.7.2", node: "24.21.0" },
    services: [], databases: [], issues: [], dns_ready: true, ca_present: true,
    active: false,
    startup: { supported: true, enabled: false, conflict: false },
    environment_autostart: false,
    binary_roles: {}, php_settings: {}, project_databases: [],
    setup: { completed: true, home_ready: true, dns_policy: true, dns_server: true, dns_system: true, ca_present: true, ca_trusted: true, caddy: true, php: true, mysql: true },
    tools: ([
      { id: "composer", version: "2.8.7", url: "", sha256: "", filename: "composer.phar", kind: "phar", binary: "composer.phar" },
      { id: "pnpm", version: "10.30.1", url: "", sha256: "", filename: "pnpm.cjs", kind: "js", binary: "pnpm.cjs" },
    ] as unknown as NonNullable<Snapshot["tools"]>),
    available_tools: [],
    tool_defaults: { composer: "2.8.7", pnpm: "10.30.1" },
    profiles: [],
  } as unknown as Snapshot;
}

describe("per-site environment surface", () => {
  it("shows only the Web Server for a static project", () => {
    const selected = site("static");
    const html = renderToStaticMarkup(<SiteEnvironmentPanel site={selected} data={snapshot(selected)} busy={false} act={act} />);
    expect(html).toContain('aria-label="Web Server version"');
    expect(html).not.toContain('aria-label="PHP version"');
    expect(html).not.toContain('aria-label="Node.js version"');
    expect(html).not.toContain("<h3>Tools</h3>");
  });

  it("shows the complete Laravel stack and catalog versions that are not installed", () => {
    const selected = site("laravel", ["php", "mysql", "node"]);
    selected.resolved = { php: "8.5.1", mysql: "9.7.2", node: "24.21.0", web_server: "caddy:2.10.2" };
    const html = renderToStaticMarkup(<SiteEnvironmentPanel site={selected} data={snapshot(selected)} busy={false} act={act} />);
    for (const label of ["Web Server version", "PHP version", "MySQL version", "Node.js version", "Composer version", "pnpm version"])
      expect(html).toContain(`aria-label="${label}"`);
    expect(html).toContain("8.6.0 · Not installed");
  });

  it("locks a database-bound MySQL version and explains the migration boundary", () => {
    const selected = site("laravel", ["php", "mysql"]);
    selected.resolved = { php: "8.5.1", mysql: "9.7.2" };
    const data = snapshot(selected);
    data.project_databases = [{ site_id: selected.id, runtime_id: "mysql:9.7.2", database_name: "demo", username: "demo", credential_ref: "test", status: "ready" }];
    const html = renderToStaticMarkup(<SiteEnvironmentPanel site={selected} data={data} busy={false} act={act} />);
    expect(html).toContain('aria-label="MySQL version" disabled=""');
    expect(html).toContain("Automatic migration is not supported");
    expect(html).toContain("demo");
  });

  it("shows profile provenance/customization and protects built-ins while custom profiles remain editable", () => {
    const selected = site("plain_php", ["php"]);
    selected.overrides = { php: "8.5.1", web_server: "caddy:2.10.2" };
    selected.resolved = { ...selected.overrides };
    selected.last_applied_profile_id = "builtin-php";
    const data = snapshot(selected);
    data.profiles = [
      { id: "builtin-php", name: "PHP", description: "Built in", bindings: { web_server: { provider: "caddy", version: "2.10.2" }, php: { provider: "php", version: "8.5.1" } }, tools: {}, built_in: true, created_at: 0, updated_at: 0 },
      { id: "custom", name: "Custom", description: "Editable", bindings: {}, tools: {}, built_in: false, created_at: 1, updated_at: 1 },
    ];
    const environment = renderToStaticMarkup(<SiteEnvironmentPanel site={selected} data={data} busy={false} act={act} />);
    expect(environment).toContain("Based on PHP");
    expect(environment).toContain("Reapply");
    selected.overrides.php = "8.6.0";
    expect(renderToStaticMarkup(<SiteEnvironmentPanel site={selected} data={data} busy={false} act={act} />)).toContain("Customized");
    const profiles = renderToStaticMarkup(<Profiles data={data} busy={false} act={act} />);
    expect(profiles.match(/>Duplicate</g)).toHaveLength(2);
    expect(profiles.match(/>Edit</g)).toHaveLength(1);
    expect(profiles.match(/>Delete</g)).toHaveLength(1);
  });
});
