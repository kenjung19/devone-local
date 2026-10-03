import { describe, it, expect } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";
import { Setup } from "./Setup";
import { PhpDialog } from "./RuntimeDialogs";
import { SiteDetail } from "./SiteDetail";
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
  it("shows incomplete real state and retry controls without reporting trust", () => {
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
    expect(html).toContain("Retry DNS");
    expect(html).toContain("ยังไม่มี trust");
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
