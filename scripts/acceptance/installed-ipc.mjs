// Run against an installed executable built with --features release-acceptance.
// The normal release excludes this transport. No browser screenshots are claimed.
import { spawn, execFileSync } from "node:child_process";
import {
  existsSync,
  mkdirSync,
  mkdtempSync,
  readFileSync,
  writeFileSync,
  renameSync,
  rmSync,
} from "node:fs";
import { resolve, join, dirname } from "node:path";
import { randomUUID } from "node:crypto";
import { setTimeout as delay } from "node:timers/promises";
import { createConnection } from "node:net";
import assert from "node:assert/strict";

const exe = resolve(process.argv[2] ?? "");
assert(
  process.argv[2] && existsSync(exe),
  "Supply an installed instrumented devone-local.exe",
);
assert(
  existsSync(join(dirname(exe), "uninstall.exe")),
  "Acceptance requires a disposable installed application",
);
const evidenceRoot = resolve(".devone-test/release-gates-20261005");
mkdirSync(evidenceRoot, { recursive: true });
const home = mkdtempSync(join(evidenceRoot, "installed-ipc-"));
const token = randomUUID();
const results = {
  home,
  executable: exe,
  transport: "real-tauri-test-only-ipc",
  visual_observation: false,
};
const env = Object.fromEntries(
  Object.entries(process.env).filter(
    ([k]) =>
      !/^DEVONE_|^(CARGO_HOME|RUSTUP_HOME|PNPM_HOME|NODE_PATH|NODE_OPTIONS|RUSTUP_TOOLCHAIN|VITE_DEV_SERVER_URL|TAURI_DEV_HOST)$/i.test(
        k,
      ),
  ),
);
env.PATH = `${env.SystemRoot ?? env.SYSTEMROOT}/System32;${env.SystemRoot ?? env.SYSTEMROOT}`;
env.DEVONE_ACCEPTANCE_TOKEN = token;
let primary,
  requestId = 0;
const runKey = "HKCU\\Software\\Microsoft\\Windows\\CurrentVersion\\Run";
const reg = join(env.SystemRoot ?? env.SYSTEMROOT, "System32/reg.exe");
function startupValue() {
  try {
    return execFileSync(reg, ["query", runKey, "/v", "DEVONE Local"], {
      encoding: "utf8",
      windowsHide: true,
      stdio: ["ignore", "pipe", "pipe"],
    })
      .match(/REG_SZ\s+([^\r\n]+)/)?.[1]
      .trim();
  } catch {
    return undefined;
  }
}
const originalStartup = startupValue();
const ownedStartup = `"${exe}" --startup --home "${home}"`;
async function poll(read, seconds = 30) {
  const end = Date.now() + seconds * 1000;
  while (Date.now() < end) {
    const value = await read();
    if (value) return value;
    await delay(100);
  }
  throw Error("Installed acceptance timed out");
}
function readJson(file) {
  try {
    return JSON.parse(readFileSync(join(home, "config", file), "utf8"));
  } catch {
    return undefined;
  }
}
async function launch(startup = false) {
  const child = spawn(
    exe,
    ["--home", home, "--acceptance-test", ...(startup ? ["--startup"] : [])],
    { cwd: dirname(exe), env, windowsHide: true, stdio: "ignore" },
  );
  const done = new Promise((ok, fail) => {
    child.on("error", fail);
    child.on("exit", (code) =>
      code === 0 ? ok() : fail(Error(`Desktop exited ${code}`)),
    );
  });
  void done.catch(() => {});
  primary = { child, done };
  await poll(() => {
    if (child.exitCode !== null)
      throw Error(`Desktop exited before readiness: ${child.exitCode}`);
    const ready = readJson("acceptance-ready.json");
    return ready?.pid === child.pid && ready;
  });
}
async function request(command, expectError = false) {
  const id = ++requestId;
  const file = join(home, "config", "acceptance-request.json");
  writeFileSync(file + ".partial", JSON.stringify({ token, id, command }));
  renameSync(file + ".partial", file);
  const response = await poll(() => {
    const value = readJson("acceptance-response.json");
    return value?.id === id && value;
  }, 240);
  if (expectError) {
    assert.equal(response.ok, false, "Expected action rejection");
    return response.error;
  }
  assert.equal(response.ok, true, response.error);
  return response.value;
}
const action = async (value) =>
  await request({ type: "execute", action: value });
const snapshot = () => request({ type: "snapshot" });
async function quit(preserveStartup = false) {
  await request({ type: "quit", preserve_startup: preserveStartup });
  await Promise.race([
    primary.done,
    delay(20000).then(() => {
      throw Error("Quit did not exit controller");
    }),
  ]);
  const stopped = readJson("acceptance-stopped.json");
  assert(stopped?.shutdown_succeeded && !stopped.dns_owned && !stopped.active);
  assert(
    stopped.services.every((s) => s.pid == null),
    "Quit must stop owned runtime processes",
  );
  assert(!existsSync(join(home, "config/desktop-session.json")));
  primary = undefined;
}
async function portClosed(port) {
  return new Promise((resolveClosed) => {
    const socket = createConnection({ host: "127.0.0.1", port });
    socket.on("connect", () => {
      socket.destroy();
      resolveClosed(false);
    });
    socket.on("error", () => resolveClosed(true));
    socket.setTimeout(1000, () => {
      socket.destroy();
      resolveClosed(false);
    });
  });
}
try {
  await launch();
  let state = await snapshot();
  assert(
    state.setup.home_ready &&
      !state.setup.completed &&
      !state.setup.caddy &&
      !state.setup.php &&
      !state.setup.mysql,
  );
  assert(
    state.setup.dns_server,
    "Exclusive port 53 is required for desktop resolver acceptance",
  );
  results.initial_home_and_readiness = true;
  for (const [kind, version] of [
    ["caddy", "2.11.7"],
    ["php", "8.4.26"],
  ]) {
    await action({ type: "install", runtime: { kind, version } });
    state = (await action({ type: "default", runtime: { kind, version } }))
      .snapshot;
    assert(state.setup[kind]);
  }
  results.catalog_runtime_install = true;
  const phpFolder = join(home, "www", "ipc-php");
  mkdirSync(phpFolder);
  writeFileSync(join(phpFolder, "index.php"), "<?php echo PHP_VERSION;");
  writeFileSync(join(phpFolder, ".env"), "USER_FILE=preserve\n");
  state = await poll(async () => {
    const s = await snapshot();
    return s.sites.some((site) => site.name === "ipc-php") && s;
  });
  const site = state.sites.find((s) => s.name === "ipc-php");
  await action({
    type: "override",
    site_id: site.id,
    kind: "php",
    version: "8.4.26",
  });
  state = (
    await action({ type: "site_action", site_id: site.id, operation: "start" })
  ).snapshot;
  assert(state.services.some((s) => s.key === "php:8.4.26" && s.healthy));
  await action({ type: "site_action", site_id: site.id, operation: "stop" });
  results.discovery_override_start_stop = true;
  const mysqlSource = process.env.DEVONE_MYSQL_A_SOURCE;
  const mysqlManifest = state.available.find(
    (m) => m.runtime === "mysql" && m.version === "8.4.11",
  );
  if (mysqlSource)
    await action({
      type: "import",
      source: resolve(mysqlSource),
      manifest: mysqlManifest,
    });
  else
    await action({
      type: "install",
      runtime: { kind: "mysql", version: "8.4.11" },
    });
  state = (
    await action({
      type: "default",
      runtime: { kind: "mysql", version: "8.4.11" },
    })
  ).snapshot;
  assert.equal(
    state.setup.mysql,
    false,
    "Installed uninitialized MySQL must not be ready",
  );
  state = (
    await action({
      type: "database",
      runtime: { kind: "mysql", version: "8.4.11" },
      operation: "initialize",
    })
  ).snapshot;
  assert(state.setup.mysql);
  await action({
    type: "override",
    site_id: site.id,
    kind: "mysql",
    version: "8.4.11",
  });
  state = (
    await action({
      type: "provision",
      site_id: site.id,
      database_name: "ipc_acceptance",
    })
  ).snapshot;
  assert(
    state.project_databases.some(
      (b) => b.site_id === site.id && b.runtime_id === "mysql:8.4.11",
    ),
  );
  assert.equal(
    readFileSync(join(phpFolder, ".env"), "utf8"),
    "USER_FILE=preserve\n",
  );
  results.database_initialize_provision = true;
  await action({
    type: "install_tool",
    id: "composer",
    version: "2.10.3",
    node: null,
  });
  const validated = await action({
    type: "tool_action",
    id: "composer",
    version: "2.10.3",
    operation: "validate",
    runtime_version: "8.4.26",
  });
  assert(validated.message?.includes("2.10.3"));
  results.tool_action_selected_php = true;
  await action({
    type: "create_project",
    request: {
      name: "ipc-created",
      template: "static",
      runtimes: {},
      tools: {},
      install_dependencies: false,
      database_name: null,
      configure_mail: false,
    },
  });
  state = await poll(async () => {
    const s = await snapshot();
    const t = s.developer.creation_tasks.find(
      (t) => t.project_name === "ipc-created",
    );
    if (t?.status === "failed") throw Error(t.error);
    return t?.status === "completed" && s;
  }, 60);
  assert(state.sites.some((s) => s.name === "ipc-created"));
  results.new_project_action = true;
  await action({ type: "environment_autostart", enabled: false });
  await request(
    { type: "execute", action: { type: "finish_setup", skip: false } },
    true,
  );
  assert(!(await snapshot()).setup.completed);
  await action({ type: "finish_setup", skip: true });
  assert((await snapshot()).setup.completed);
  await action({ type: "reopen_setup" });
  assert(!(await snapshot()).setup.completed);
  await action({ type: "finish_setup", skip: true });
  results.first_run_rejects_incomplete_skip_reopen = true;
  await request({ type: "close_window" });
  const hidden = join(home, "www", "hidden-watcher");
  mkdirSync(hidden);
  writeFileSync(
    join(hidden, "index.html"),
    "<h1>Watcher while actual Tauri window hidden</h1>",
  );
  state = await poll(async () => {
    const s = await snapshot();
    return s.sites.some((s) => s.name === "hidden-watcher") && s;
  });
  assert(state.setup.dns_server && state.active);
  results.close_to_tray_backend_and_hidden_watcher = true;
  const beforeActivation = await request({ type: "desktop_state" });
  assert.equal(beforeActivation.window_visible, false);
  await new Promise((ok, fail) => {
    const second = spawn(exe, ["--home", home], {
      cwd: dirname(exe),
      env,
      windowsHide: true,
      stdio: "ignore",
    });
    second.on("error", fail);
    second.on("exit", (code) =>
      code === 0 ? ok() : fail(Error(`Second launch exited ${code}`)),
    );
  });
  const activation = await poll(async () => {
    const s = await request({ type: "desktop_state" });
    return s.activation_count > beforeActivation.activation_count && s;
  });
  assert(
    activation.window_visible &&
      activation.show_request_succeeded &&
      activation.focus_request_succeeded,
  );
  assert.equal(activation.controller_pid, primary.child.pid);
  results.activation_signal_show_focus_request = true;
  state = await snapshot();
  const editor = state.developer.editors.find((e) => e.id === "vscode");
  if (editor) {
    await action({
      type: "save_editor",
      editor: {
        ...editor,
        id: "custom-acceptance-vscode",
        name: "Acceptance VS Code",
        args: ["--new-window", "{project}"],
      },
    });
    await action({
      type: "editor",
      operation: "default",
      site_id: null,
      editor_id: "custom-acceptance-vscode",
    });
    await action({
      type: "editor",
      operation: "open",
      site_id: site.id,
      editor_id: "custom-acceptance-vscode",
    });
    results.detected_editor_backend_launch = true;
  } else results.detected_editor_backend_launch = "not installed";
  results.startup_combinations = [];
  if (originalStartup === undefined) {
    for (const windows of [false, true])
      for (const environment of [false, true]) {
        await action({ type: "startup", enabled: windows });
        state = (
          await action({ type: "environment_autostart", enabled: environment })
        ).snapshot;
        assert.equal(state.startup.enabled, windows);
        assert.equal(state.environment_autostart, environment);
        assert.equal(startupValue(), windows ? ownedStartup : undefined);
        await quit(true);
        await launch(windows);
        state = await snapshot();
        assert.equal(state.startup.enabled, windows);
        assert.equal(state.environment_autostart, environment);
        assert.equal(state.active, environment);
        assert(
          state.setup.dns_server &&
            state.project_databases.some((b) => b.site_id === site.id),
        );
        results.startup_combinations.push({
          windows,
          environment,
          cold_reopen: true,
        });
      }
  } else
    results.startup_combinations =
      "Existing unrelated startup entry preserved; acceptance not executed";
  state = await snapshot();
  const ports = [
    ...new Set([53, ...state.services.map((s) => s.port).filter(Boolean)]),
  ];
  await quit();
  for (const port of ports)
    assert(await portClosed(port), `Owned listener ${port} remains after Quit`);
  results.quit_stops_controller_services_and_resolver = true;
  assert.equal(startupValue(), originalStartup);
  results.startup_cleanup_preserved_baseline = true;
  writeFileSync(join(home, "results.json"), JSON.stringify(results, null, 2));
  console.log(JSON.stringify(results, null, 2));
} catch (error) {
  results.failure = { message: error.message, stack: error.stack };
  writeFileSync(join(home, "results.json"), JSON.stringify(results, null, 2));
  throw error;
} finally {
  if (primary?.child.exitCode === null) {
    try {
      await quit();
    } catch {
      primary?.child.kill();
    }
  }
  if (originalStartup === undefined && startupValue() === ownedStartup)
    execFileSync(reg, ["delete", runKey, "/v", "DEVONE Local", "/f"], {
      windowsHide: true,
      stdio: "ignore",
    });
  rmSync(join(home, "config", "acceptance-request.json.partial"), {
    force: true,
  });
}
