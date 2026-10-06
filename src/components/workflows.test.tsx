import { RuntimePanel } from "./RuntimePanel";
import { ErrorNotice } from "./ErrorNotice";
import { SitesOverview } from "./SitesOverview";
import { siteProblems, initialVersions, versionLabel } from "../presentation";
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

const act = async () => {};
describe("desktop setup and configuration presentation", () => {
  it("offers an explicit non-modal CA upgrade only for legacy/pending installs", () => {
    const legacy = { ...data, setup: { ...data.setup, caddy: true, ca_present: true, ca_trusted: true, ca_upgrade_pending: true } };
    const settings = renderToStaticMarkup(<ProductSettings data={legacy} busy={false} act={act} />);
    expect(settings).toContain("Upgrade HTTPS certificate authority");
    expect(settings).toContain("existing CA stays trusted");
    expect(settings).not.toContain('role="dialog"');
    const setup = renderToStaticMarkup(<Setup data={legacy} busy={false} error="" progress={null} act={act} importRuntime={() => {}} close={() => {}} initialStep={5} />);
    expect(setup).toContain("Upgrade HTTPS certificate authority");
    const sites = renderToStaticMarkup(<SitesOverview data={legacy} busy={false} act={act} view={() => {}} create={() => {}} />);
    expect(sites).toContain("Upgrade HTTPS certificate authority");
    expect(renderToStaticMarkup(<ProductSettings data={data} busy={false} act={act} />)).not.toContain("Upgrade HTTPS certificate authority");
  });
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

describe("daily Windows workflows", () => {
  it("offers copy/create in an empty workspace without Add Site", () => {
    const html = renderToStaticMarkup(
      <SitesOverview
        data={{ ...data, sites: [] }}
        busy={false}
        act={act}
        view={() => {}}
        create={() => {}}
      />,
    );
    expect(html).toContain("No projects yet.");
    expect(html).toContain("Open www");
    expect(html).toContain("New Project");
    expect(html).not.toContain("Add Site");
  });
  it("shows PHP and Node on hybrid projects and a sole editor without a default requirement", () => {
    const html = renderToStaticMarkup(
      <SitesOverview
        data={
          {
            ...data,
            sites: [{ ...site, resolved: { php: "8.5.1", node: "24.21.0" } }],
            developer: {
              templates: [],
              template_error: null,
              creation_tasks: [],
              default_editor: null,
              managed_databases: [],
              backups: [],
              mail: {
                installed: false,
                running: false,
                smtp_port: null,
                web_port: null,
              },
              database_catalog: {},
              backup_preferences: { automatic: false, keep_last: 7 },
              diagnostic_report: null,
              preferences: {},
              editors: [
                {
                  id: "vscode",
                  name: "VS Code",
                  category: "editor",
                  executable: "C:/code.exe",
                  args: [],
                },
              ],
            },
          } as Snapshot
        }
        busy={false}
        act={act}
        view={() => {}}
        create={() => {}}
      />,
    );
    expect(html).toContain("PHP 8.5.1 / Node 24.21.0");
    expect(html).toContain("Open in VS Code");
    expect(html).toContain("Terminal");
    expect(html).not.toContain("MySQL 8");
  });
  it("offers only an exact available missing-runtime action", () => {
    const s = { ...site, resolved: { php: "7.4.33" } };
    const p = siteProblems(s, {
      ...data,
      available: [
        {
          ...php.manifest,
          version: "7.4.33",
          download: "https://example.test/php.zip",
          sha256: "a".repeat(64),
        },
      ],
    });
    expect(p[0].action).toEqual({
      type: "install",
      runtime: { kind: "php", version: "7.4.33" },
    });
    expect(siteProblems(s, data)[0].action).toBeUndefined();
  });
  it("keeps an older selected default and displays catalog channels without upgrading", () => {
    expect(initialVersions({ ...data, defaults: { php: "7.4.33" } }).php).toBe(
      "7.4.33",
    );
    expect(
      versionLabel({
        ...php.manifest,
        runtime: "node",
        version: "24.21.0",
        metadata: { channel: "LTS" },
      }),
    ).toBe("Node 24.21.0 LTS");
  });
  it("does not offer default selection for a service tool", () => {
    const html = renderToStaticMarkup(
      <ToolManager
        data={{
          ...data,
          tools: [
            {
              id: "mailpit",
              version: "1.31.3",
              url: "",
              sha256: "",
              entry: "",
            },
          ],
          tool_defaults: { mailpit: "1.31.3" },
        }}
        busy={false}
        act={act}
      />,
    );
    expect(html).toContain("Local mail service - Stopped");
    expect(html).toContain("Open Mailbox");
    expect(html).not.toContain("Set default");
  });
});

describe("release acceptance screen structure (SSR, no viewport/layout claim)", () => {
  const complete = {
    ...data,
    developer,
    setup: {
      ...data.setup,
      home_ready: true,
      caddy: true,
      php: true,
      mysql: true,
      dns_policy: true,
      dns_server: true,
      dns_system: true,
      ca_present: true,
      ca_trusted: true,
    },
  };
  const setup = (snapshot: Snapshot, step: number, busy = false) =>
    renderToStaticMarkup(
      <Setup
        data={snapshot}
        initialStep={step}
        busy={busy}
        error=""
        progress={null}
        act={act}
        importRuntime={() => {}}
        close={() => {}}
      />,
    );
  it.each([
    [0, "Home"],
    [1, "Web Server"],
    [2, "PHP"],
    [3, "MySQL"],
    [4, "Automatic .test domains"],
    [5, "Windows trust"],
    [6, "setup-summary"],
  ])(
    "renders setup step %s with its stage and persistent navigation",
    (step, label) => {
      const html = setup(complete, step as number);
      expect(html).toContain(label);
      expect(html.match(/aria-current="step"/g)).toHaveLength(1);
      expect(html).toContain('class="setup-controls"');
      expect(html).not.toContain('role="dialog"');
    },
  );
  it.each([
    "home_ready",
    "caddy",
    "php",
    "mysql",
    "dns_policy",
    "dns_server",
    "dns_system",
    "ca_present",
    "ca_trusted",
  ] as const)("does not enable completion when backend %s is false", (key) => {
    const html = setup(
      { ...complete, setup: { ...complete.setup, [key]: false } },
      6,
    );
    const finish = html.match(
      /<button[^>]*class="primary"[^>]*>[^<]*เสร็จสิ้นและเปิด environment<\/button>/,
    )?.[0];
    expect(finish).toContain('disabled=""');
    expect(html).toContain("ข้ามส่วนที่ยังไม่พร้อม");
  });
  it("enables completion only with all readiness checks and disables it while busy", () => {
    const button = (html: string) =>
      html.match(
        /<button[^>]*class="primary"[^>]*>[^<]*เสร็จสิ้นและเปิด environment<\/button>/,
      )?.[0];
    expect(button(setup(complete, 6))).not.toContain('disabled=""');
    expect(button(setup(complete, 6, true))).toContain('disabled=""');
  });
  it("renders Sites empty, Ready, Needs setup and Error states", () => {
    const overview = (snapshot: Snapshot) =>
      renderToStaticMarkup(
        <SitesOverview
          data={snapshot}
          busy={false}
          act={act}
          view={() => {}}
          create={() => {}}
        />,
      );
    expect(overview({ ...complete, sites: [] })).toContain("No projects yet.");
    expect(overview(complete)).toContain("Ready");
    expect(overview({ ...complete, installed: [] })).toContain("Needs setup");
    expect(
      overview({ ...complete, sites: [{ ...site, status: "failed" }] }),
    ).toContain("Error");
  });
  it("renders primary workflow controls for Runtimes, Database, Tools and Settings", () => {
    const html = renderToStaticMarkup(
      <>
        <RuntimePanel
          data={complete}
          busy={false}
          act={act}
          importRuntime={() => {}}
          configure={() => {}}
        />
        <DatabaseManager
          data={complete}
          busy={false}
          act={act}
          logs={() => {}}
        />
        <ToolManager data={complete} busy={false} act={act} />
        <ProductSettings data={complete} busy={false} act={act} />
        <DeveloperSettings data={complete} busy={false} act={act} />
      </>,
    );
    for (const label of [
      "PHP",
      "Node",
      "MySQL",
      "Web Server",
      "Configure",
      "Installed",
      "Available",
      "Start DEVONE Local with Windows",
      "Start environment automatically",
      "Recreate CA",
    ])
      expect(html).toContain(label);
  });
  it.each([
    "DNS UDP 53 conflict",
    "The exact DEVONE CA is still trusted",
    "default PHP is missing",
  ])(
    "keeps understandable error guidance separate from details: %s",
    (error) => {
      const html = renderToStaticMarkup(<ErrorNotice error={error} />);
      expect(html).toContain('role="alert"');
      expect(html).toContain("<strong>");
      expect(html).toContain("<details><summary>Technical details</summary>");
      expect(html).toContain(`<pre>${error}</pre>`);
    },
  );
});
