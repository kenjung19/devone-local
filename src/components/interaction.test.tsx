// @vitest-environment jsdom
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { ImportDialog, PhpDialog } from "./RuntimeDialogs";
import { DatabaseManager } from "./DatabaseManager";
import { SiteDetail } from "./SiteDetail";
import App from "../App";
import { ProductSettings } from "./ProductSettings";
import { bridge } from "../bridge";
import type { Installation, Manifest, Site, Snapshot } from "../contracts";

vi.mock("../bridge", () => ({
  desktop: true,
  bridge: {
    execute: vi.fn(),
    snapshot: vi.fn(),
    inspectImport: vi.fn(),
    progress: vi.fn(),
  },
}));
vi.mock("@tauri-apps/api/event", () => ({ listen: async () => () => {} }));
vi.mock("@tauri-apps/api/webview", () => ({
  getCurrentWebview: () => ({ onDragDropEvent: async () => () => {} }),
}));
Object.assign(globalThis, { IS_REACT_ACT_ENVIRONMENT: true });
const data: Snapshot = {
  home: "D:/home",
  platform: "windows-x64",
  sites: [],
  installed: [],
  available: [],
  defaults: {},
  services: [],
  databases: [],
  issues: [],
  dns_ready: false,
  ca_present: false,
  active: false,
  startup: { supported: true, enabled: false, conflict: false },
  environment_autostart: false,
  binary_roles: { php: { cli: "php.exe", fastcgi: "php-cgi.exe" } },
  php_settings: {},
  project_databases: [],
  setup: {
    completed: true,
    home_ready: true,
    dns_policy: false,
    dns_server: false,
    dns_system: false,
    ca_present: false,
    ca_trusted: false,
    caddy: false,
    php: false,
    mysql: false,
  },
};
const runtime: Installation = {
  id: "php:8.4.0",
  manifest: {
    runtime: "php",
    version: "8.4.0",
    platform: data.platform,
    binaries: data.binary_roles.php,
    metadata: {},
  },
  relative_path: "runtimes/php/8.4.0",
  installed_at: 1,
};
let host: HTMLDivElement;
let root: Root;
beforeEach(() => {
  vi.clearAllMocks();
  host = document.createElement("div");
  document.body.append(host);
  root = createRoot(host);
});
afterEach(async () => {
  await act(async () => root.unmount());
  host.remove();
  vi.useRealTimers();
});
async function click(text: string) {
  const button = [...host.querySelectorAll("button")].find(
    (b) => b.textContent?.trim() === text,
  );
  expect(button, text).toBeDefined();
  await act(async () => button!.click());
}
async function input(placeholder: string, value: string) {
  const element = [...host.querySelectorAll("input")].find(
    (i) => i.placeholder === placeholder,
  )!;
  await act(async () => {
    Object.getOwnPropertyDescriptor(
      HTMLInputElement.prototype,
      "value",
    )!.set!.call(element, value);
    element.dispatchEvent(new Event("input", { bubbles: true }));
  });
}
describe("runtime dialogs", () => {
  it("a stale inspection cannot finish a newer inspection", async () => {
    const results: ((m: Manifest) => void)[] = [];
    vi.mocked(bridge.inspectImport).mockImplementation(
      () => new Promise((ok) => results.push(ok)),
    );
    await act(async () =>
      root.render(
        <ImportDialog
          kind="php"
          data={data}
          busy={false}
          error=""
          close={() => {}}
          act={async () => true}
        />,
      ),
    );
    await input("Absolute path to extracted distribution", "D:/first");
    await click("Detect version");
    await input("Absolute path to extracted distribution", "D:/second");
    await click("Detect version");
    await act(async () => results[0](runtime.manifest));
    expect(
      [...host.querySelectorAll("button")].find(
        (b) => b.textContent === "Detecting…",
      )?.disabled,
    ).toBe(true);
    await act(async () =>
      results[1]({ ...runtime.manifest, version: "8.5.0" }),
    );
    expect(
      host.querySelector<HTMLInputElement>(
        'input[placeholder="Version reported by --version"]',
      )!.value,
    ).toBe("8.5.0");
  });
  it.each([false, true])(
    "closes Import and PHP only after success=%s",
    async (success) => {
      const close = vi.fn();
      const execute = vi.fn(async () => success);
      for (const dialog of [
        <ImportDialog
          kind="php"
          data={data}
          busy={false}
          error=""
          close={close}
          act={execute}
        />,
        <PhpDialog
          runtime={runtime}
          data={data}
          busy={false}
          error=""
          close={close}
          act={execute}
        />,
      ]) {
        await act(async () => root.render(dialog));
        await act(async () =>
          host
            .querySelector("form")!
            .dispatchEvent(
              new Event("submit", { bubbles: true, cancelable: true }),
            ),
        );
      }
      expect(execute).toHaveBeenCalledTimes(2);
      expect(close).toHaveBeenCalledTimes(success ? 2 : 0);
    },
  );
  it("ignores version inspection after the source changes", async () => {
    let resolve!: (m: Manifest) => void;
    vi.mocked(bridge.inspectImport).mockImplementation(
      () =>
        new Promise((ok) => {
          resolve = ok;
        }),
    );
    await act(async () =>
      root.render(
        <ImportDialog
          kind="php"
          data={data}
          busy={false}
          error=""
          close={() => {}}
          act={async () => true}
        />,
      ),
    );
    await input("Absolute path to extracted distribution", "D:/old");
    await click("Detect version");
    await input("Absolute path to extracted distribution", "D:/new");
    await act(async () => resolve(runtime.manifest));
    expect(
      host.querySelector<HTMLInputElement>(
        'input[placeholder="Version reported by --version"]',
      )!.value,
    ).toBe("");
  });
});
describe("credential and polling state", () => {
  it("disables reveal while pending and clears passwords when managed records change", async () => {
    let resolve!: (value: Awaited<ReturnType<typeof bridge.execute>>) => void;
    vi.mocked(bridge.execute).mockImplementation(
      () =>
        new Promise((ok) => {
          resolve = ok;
        }),
    );
    const managed = [
      {
        runtime_id: "mysql:8.4",
        database_name: "demo",
        username: "demo",
        status: "ready",
      },
    ];
    const developer = {
      managed_databases: managed,
      backups: [],
      editors: [],
    } as unknown as NonNullable<Snapshot["developer"]>;
    const render = async (value: typeof developer) =>
      act(async () =>
        root.render(
          <DatabaseManager
            data={{ ...data, developer: value }}
            busy={false}
            act={async () => true}
            logs={() => {}}
          />,
        ),
      );
    await render(developer);
    await click("Reveal Password");
    expect(
      [...host.querySelectorAll("button")].find(
        (b) => b.textContent === "Reveal Password",
      )?.disabled,
    ).toBe(true);
    await act(async () =>
      resolve({ snapshot: data, message: null, credential: "pending-secret" }),
    );
    expect(host.textContent).toContain("pending-secret");
    await render({
      ...developer,
      managed_databases: [...developer.managed_databases],
    });
    expect(host.textContent).not.toContain("pending-secret");
    await click("Reveal Password");
    await render({ ...developer, managed_databases: [] });
    await act(async () =>
      resolve({ snapshot: data, message: null, credential: "stale-secret" }),
    );
    expect(host.textContent).not.toContain("stale-secret");
  });
  it("shows blocked CA recovery only in that state and requires confirmation", async () => {
    const execute = vi.fn(async () => true);
    const confirm = vi.spyOn(window, "confirm").mockReturnValue(false);
    const blocked = {
      ...data,
      setup: { ...data.setup, caddy: true, ca_recovery_needed: true },
    };
    await act(async () =>
      root.render(
        <ProductSettings data={blocked} busy={false} act={execute} />,
      ),
    );
    await click("Abandon upgrade and create new CA");
    expect(execute).not.toHaveBeenCalled();
    confirm.mockReturnValue(true);
    await click("Abandon upgrade and create new CA");
    expect(execute).toHaveBeenCalledWith({
      type: "recover_ca",
      confirmed: true,
    });
    await act(async () =>
      root.render(<ProductSettings data={data} busy={false} act={execute} />),
    );
    expect(host.textContent).not.toContain("Abandon upgrade and create new CA");
    confirm.mockRestore();
  });
  it("reveals a DB password locally and erases it on Hide", async () => {
    const execute = vi.fn(async () => true);
    vi.mocked(bridge.execute).mockResolvedValue({
      snapshot: data,
      message: null,
      credential: "private-password",
    });
    const developer = {
      managed_databases: [
        {
          runtime_id: "mysql:8.4",
          database_name: "demo",
          username: "demo",
          status: "ready",
        },
      ],
      backups: [],
      editors: [],
    } as unknown as NonNullable<Snapshot["developer"]>;
    await act(async () =>
      root.render(
        <DatabaseManager
          data={{ ...data, developer }}
          busy={false}
          act={execute}
          logs={() => {}}
        />,
      ),
    );
    await click("Reveal Password");
    expect(host.textContent).toContain("private-password");
    expect(execute).not.toHaveBeenCalled();
    await click("Hide");
    expect(host.textContent).not.toContain("private-password");
  });
  it("clears a failed poll when the next refresh succeeds", async () => {
    vi.useFakeTimers();
    vi.mocked(bridge.snapshot)
      .mockRejectedValueOnce(Error("temporary poll failure"))
      .mockResolvedValue(data);
    await act(async () => root.render(<App />));
    expect(host.textContent).toContain("temporary poll failure");
    await act(async () => vi.advanceTimersByTimeAsync(4000));
    expect(host.textContent).not.toContain("temporary poll failure");
    expect(host.textContent).toContain("Sites");
  });
});
describe("process confirmation", () => {
  it("cancel prevents replacement and removal; confirmation submits each", async () => {
    const execute = vi.fn(async () => true);
    const confirm = vi.spyOn(window, "confirm").mockReturnValue(false);
    const site: Site = {
      id: "demo",
      name: "demo",
      hostname: "demo.test",
      project_path: "D:/home/www/demo",
      project_type: "node",
      document_root: "D:/home/www/demo",
      present: true,
      issue: null,
      overrides: {},
      resolved: {},
      status: "stopped",
      https: "unavailable",
      discovered_at: 1,
      updated_at: 1,
      metadata: {
        scripts: ["dev"],
        route: "node_proxy",
        requirements: [],
        runtimes: {},
        framework: "node",
        package_manager: null,
        package_manager_version: null,
        dev_script: "dev",
        build_script: null,
        node_dependencies: true,
        composer_dependencies: false,
        error: null,
      },
      processes: [
        {
          site_id: "demo",
          key: "site:demo:web",
          enabled: false,
          status: "stopped",
          health_strategy: "http",
          definition: {
            id: "web",
            name: "Web server",
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
    await act(async () =>
      root.render(
        <SiteDetail
          site={site}
          data={data}
          busy={false}
          act={execute}
          logs={() => {}}
          back={() => {}}
        />,
      ),
    );
    await click("Use dev script (Local override)");
    await click("Reset definition");
    expect(execute).not.toHaveBeenCalled();
    confirm.mockReturnValue(true);
    await click("Use dev script (Local override)");
    await click("Reset definition");
    expect(execute).toHaveBeenCalledWith(
      expect.objectContaining({ type: "save_process", replace: true }),
    );
    expect(execute).toHaveBeenCalledWith(
      expect.objectContaining({ type: "site_process", operation: "remove" }),
    );
    confirm.mockRestore();
  });
});
