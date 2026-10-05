import { NewProject } from "./NewProject";
import { DatabaseManager } from "./DatabaseManager";
import { DeveloperSettings } from "./DeveloperSettings";
import { ToolManager } from "./ToolManager";
import { describe, it, expect } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";
import { Setup } from "./Setup";
import { PhpDialog } from "./RuntimeDialogs";
import { SiteDetail } from "./SiteDetail";
import { ProductSettings } from "./ProductSettings";
import type { Snapshot, Site, Installation } from "../contracts";
const php: Installation = {
  id: "php:8.5.1",
  manifest: {
    runtime: "php",
    version: "8.5.1",
    platform: "windows-x64",
    binaries: { cli: "php.exe", fastcgi: "php-cgi.exe" },
    metadata: {},
  },
  relative_path: "runtimes/php/8.5.1",
  installed_at: 1,
};
const site: Site = {
  id: "project-id",
  name: "project",
  hostname: "project.test",
  project_path: "D:/home/www/project",
  project_type: "plain_php",
  document_root: "D:/home/www/project",
  present: true,
  issue: null,
  overrides: {},
  resolved: { php: "8.5.1" },
  status: "stopped",
  https: "unavailable",
  discovered_at: 1,
  updated_at: 1,
};
const data: Snapshot = {
  home: "D:/home",
  platform: "windows-x64",
  sites: [site],
  installed: [php],
  available: [{ ...php.manifest, version: "8.6.0" }],
  defaults: { php: "8.5.1" },
  services: [],
  databases: [],
  issues: [],
  dns_ready: false,
  ca_present: false,
  active: false,
  startup: { supported: true, enabled: false, conflict: false },
  environment_autostart: false,
  binary_roles: { php: { cli: "php.exe", fastcgi: "php-cgi.exe" } },
  php_settings: {
    "8.5.1": {
      config: { directives: { memory_limit: "256M" }, extensions: [] },
      available_extensions: ["php_curl.dll"],
      original_ini: true,
    },
  },
  project_databases: [],
  setup: {
    completed: false,
    home_ready: true,
    dns_policy: false,
    dns_server: false,
    dns_system: false,
    ca_present: false,
    ca_trusted: false,
    caddy: false,
    php: true,
    mysql: false,
  },
};
const act = async () => {};
describe("desktop setup and configuration presentation", () => {
  it("keeps login and environment preferences separate and does not mark a stopped resolver ready", () => {
    const html = renderToStaticMarkup(
      <ProductSettings
        data={{
          ...data,
          startup: { supported: true, enabled: true, conflict: false },
          environment_autostart: false,
          setup: {
            ...data.setup,
            dns_policy: true,
            dns_system: true,
            dns_server: false,
          },
        }}
        busy={false}
        act={act}
      />,
    );
    expect(html).toContain("Start DEVONE Local with Windows");
    expect(html).toContain("Start environment automatically");
    expect(html.match(/checked=""/g)).toHaveLength(1);
    expect(html).toContain("Stopped");
    expect(html).toContain("Not ready");
    expect(html).toContain("Remove DEVONE DNS integration");
    expect(html).toContain("Recreate CA");
  });
  it("retains a missing explicit PHP version and offers an install action only for that version", () => {
    const manifest = {
      ...php.manifest,
      version: "7.3.33",
      download: "https://example.test/php.zip",
      sha256: "a".repeat(64),
    };
    const html = renderToStaticMarkup(
      <SiteDetail
        site={{
          ...site,
          overrides: { php: "7.3.33" },
          resolved: { php: "7.3.33" },
        }}
        data={{ ...data, available: [manifest] }}
        busy={false}
        act={act}
        back={() => {}}
        logs={() => {}}
      />,
    );
    expect(html).toContain("requires PHP 7.3.33");
    expect(html).toContain("Install PHP 7.3.33");
    expect(html).toContain('value="7.3.33"');
    expect(html).toContain("Not installed");
    expect(html).toContain("Change Runtime");
  });
  it("shows the first setup step and errors as a full page without premature trust", () => {
    const html = renderToStaticMarkup(
      <Setup
        data={data}
        busy={false}
        error="DNS TCP 53 conflict"
        progress={null}
        act={act}
        importRuntime={() => {}}
        close={() => {}}
      />,
    );
    expect(html).toContain("DNS TCP 53 conflict");
    expect(html).toContain('class="setup-page"');
    expect(html).not.toContain('class="overlay"');
    expect(html).not.toContain('class="dialog');
    expect(html).toContain("ขั้นตอน 1 จาก 7");
    expect(html).toContain("เปิด www");
    expect(html).not.toContain("Windows เชื่อถือแล้ว");
    expect(html).toContain("disabled");
  });
  it("prefills saved PHP directives and exposes only discovered extension files", () => {
    const html = renderToStaticMarkup(
      <PhpDialog
        runtime={php}
        data={data}
        busy={false}
        error="candidate failed"
        close={() => {}}
        act={act}
      />,
    );
    expect(html).toContain('value="256M"');
    expect(html).toContain("php_curl.dll");
    expect(html).not.toContain("php_imaginary.dll");
    expect(html).toContain("candidate failed");
    expect(html).not.toContain("<textarea");
  });
  it("offers installed versions for site bindings and includes service/database entry points", () => {
    const html = renderToStaticMarkup(
      <SiteDetail
        site={site}
        data={data}
        busy={false}
        act={act}
        back={() => {}}
        logs={() => {}}
      />,
    );
    expect(html).toContain('value="8.5.1"');
    expect(html).not.toContain('value="8.6.0"');
    expect(html).toContain("Global default");
    expect(html).toContain("Logs");
    expect(html).toContain("Project database");
  });
});

describe("Phase 2 site controls", () => {
  it("keeps Node-only selectors and processes free of PHP/MySQL controls", () => {
    const node: Site = {
      ...site,
      project_type: "next",
      resolved: { node: "24.21.0" },
      metadata: {
        framework: "next",
        requirements: ["node"],
        runtimes: {},
        package_manager: "pnpm",
        package_manager_version: "12.8.1",
        dev_script: "dev",
        build_script: "build",
        route: "node_proxy",
        node_dependencies: false,
        composer_dependencies: false,
        error: null,
      },
      processes: [
        {
          site_id: site.id,
          key: `site:${site.id}:web`,
          enabled: false,
          definition: {
            id: "web",
            name: "Web",
            runtime: "node",
            executable: "pnpm",
            args: ["run", "dev"],
            cwd: ".",
            env: {},
            port: true,
            autostart: true,
          },
        },
      ],
    };
    const html = renderToStaticMarkup(
      <SiteDetail
        site={node}
        data={data}
        busy={false}
        act={act}
        back={() => {}}
        logs={() => {}}
      />,
    );
    expect(html).toContain("runtime-node");
    expect(html).not.toContain("runtime-php");
    expect(html).not.toContain("runtime-mysql");
    expect(html).not.toContain("Project database");
    expect(html).toContain("node_modules missing");
    expect(html).toContain("Install pnpm dependencies");
    expect(html).toContain("disabled");
    expect(html).toContain("start site");
  });
  it("honestly marks npm projects and provides static controls without runtime selectors", () => {
    const staticSite: Site = {
      ...site,
      project_type: "static",
      resolved: {},
      metadata: {
        framework: "static",
        requirements: [],
        runtimes: {},
        package_manager: null,
        package_manager_version: null,
        dev_script: null,
        build_script: null,
        route: "static",
        node_dependencies: false,
        composer_dependencies: false,
        error: null,
      },
    };
    const html = renderToStaticMarkup(
      <SiteDetail
        site={staticSite}
        data={data}
        busy={false}
        act={act}
        back={() => {}}
        logs={() => {}}
      />,
    );
    expect(html).not.toContain("runtime-php");
    expect(html).not.toContain("runtime-node");
    expect(html).toContain("start site");
  });
});

describe("Modern workflow UX", () => {
  it("shows exact tool versions and default management", () => {
    const html = renderToStaticMarkup(
      <ToolManager
        data={{
          ...data,
          tool_defaults: { pnpm: "10.30.1" },
          tools: [
            { id: "pnpm", version: "10.30.1", url: "", sha256: "", entry: "" },
          ],
          available_tools: [
            { id: "pnpm", version: "10.30.1", url: "", sha256: "", entry: "" },
            { id: "pnpm", version: "11.0.0", url: "", sha256: "", entry: "" },
          ],
        }}
        busy={false}
        act={act}
      />,
    );
    expect(html).toContain("pnpm 10.30.1");
    expect(html).toContain("pnpm 11.0.0");
    expect(html).toContain("Set default");
    expect(html).toContain("Validate");
    expect(html).toContain("Remove");
  });
  it("distinguishes portable runtime source and process groups", () => {
    const html = renderToStaticMarkup(
      <SiteDetail
        site={{
          ...site,
          project_type: "node",
          resolved: { node: "24.21.0" },
          overrides: { node: "24.21.0" },
          runtime_sources: { node: ".devone.json" },
        }}
        data={data}
        busy={false}
        act={act}
        back={() => {}}
        logs={() => {}}
      />,
    );
    expect(html).toContain(".devone.json");
    expect(html).toContain("Frontend");
    expect(html).toContain("Workers");
    expect(html).toContain("Custom");
    expect(html).toContain("Save portable config");
    expect(html).not.toContain("Project database");
  });
});

describe("Portable config errors", () => {
  it("shows invalid config details even on a static site without tools", () => {
    const html = renderToStaticMarkup(
      <SiteDetail
        site={{
          ...site,
          project_type: "static",
          resolved: {},
          overrides: {},
          metadata: {
            framework: "static",
            requirements: [],
            runtimes: {},
            package_manager: null,
            package_manager_version: null,
            dev_script: null,
            build_script: null,
            route: "static",
            node_dependencies: false,
            composer_dependencies: false,
            error: ".devone.json contains an error: unknown field",
          },
        }}
        data={data}
        busy={false}
        act={act}
        back={() => {}}
        logs={() => {}}
      />,
    );
    expect(html).toContain(".devone.json contains an error: unknown field");
    expect(html).not.toContain("runtime-php");
    expect(html).not.toContain("runtime-node");
    expect(html).not.toContain("runtime-mysql");
  });
});

describe("Phase 3 workflows", () => {
  const developer: NonNullable<Snapshot["developer"]> = {
    templates: [
      {
        id: "static",
        name: "Static HTML",
        category: "Static",
        strategy: "static",
        version: "",
        runtimes: {},
        tools: [],
        custom: false,
      },
    ],
    template_error: null,
    creation_tasks: [],
    editors: [],
    default_editor: null,
    preferences: {},
    managed_databases: [],
    backups: [],
    mail: { installed: false, running: false, smtp_port: null, web_port: null },
    database_catalog: {},
    backup_preferences: { automatic: false, keep_last: 7 },
    diagnostic_report: null,
  };
  it("shows Static creation without irrelevant runtime or database fields", () => {
    const html = renderToStaticMarkup(
      <NewProject
        data={{ ...data, developer }}
        busy={false}
        act={async () => {}}
        view={() => {}}
      />,
    );
    expect(html).toContain("New Project");
    expect(html).toContain("Static HTML");
    expect(html).not.toContain("minimum");
    expect(html).not.toContain("Database name");
    expect(html).not.toContain("selected php");
    expect(html).not.toContain("selected node");
  });
  it("keeps system and external databases read-only", () => {
    const html = renderToStaticMarkup(
      <DatabaseManager
        data={{
          ...data,
          developer: {
            ...developer,
            database_catalog: {
              "mysql:8.4.11": [
                { name: "mysql", system: true, managed: false },
                { name: "external", system: false, managed: false },
              ],
            },
          },
          installed: [
            {
              ...php,
              id: "mysql:8.4.11",
              manifest: {
                ...php.manifest,
                runtime: "mysql",
                version: "8.4.11",
              },
            },
          ],
        }}
        busy={false}
        act={async () => {}}
        logs={() => {}}
      />,
    );
    expect(html).toContain("System databases");
    expect(html).toContain("External · read only");
    expect(html).not.toContain("Delete…");
    expect(html).not.toContain("Confirm delete");
  });
  it("shows only installed editors and separates local paths from portable intent", () => {
    const html = renderToStaticMarkup(
      <DeveloperSettings
        data={{
          ...data,
          developer: {
            ...developer,
            editors: [
              {
                id: "custom-editor",
                name: "Installed Editor",
                executable: "C:/Editor/editor.exe",
                args: ["{project}"],
                category: "editor",
              },
            ],
          },
        }}
        busy={false}
        act={async () => {}}
      />,
    );
    expect(html).toContain("Installed Editor");
    expect(html).toContain("machine-local");
    expect(html).not.toContain("Open in Cursor");
    expect(html).toContain("Automatic scheduling and deletion are not enabled");
  });
});
