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
