import { spawn } from "node:child_process";
import {
  mkdtempSync,
  mkdirSync,
  readFileSync,
  writeFileSync,
  existsSync,
} from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { setTimeout as delay } from "node:timers/promises";
import { smokeExecutable, assertSmokeAlive } from "./smoke-support.mjs";
const executable = smokeExecutable(process.argv[2]);
if (!existsSync(executable))
  throw Error(
    "Build release first or supply an installed devone-local.exe path",
  );
const home = mkdtempSync(join(tmpdir(), "devone-release-smoke-"));
const launch = () => {
  const child = spawn(executable, ["--home", home, "--smoke-test"], {
    windowsHide: true,
    stdio: "ignore",
  });
  let error;
  const done = new Promise((ok, fail) => {
    child.on("error", (value) => {
      error = value;
      fail(value);
    });
    child.on("exit", (code) =>
      code === 0 ? ok() : fail(Error(`Executable exited ${code}`)),
    );
  });
  return { child, done, check: () => assertSmokeAlive(child, error) };
};
async function poll(read) {
  const end = Date.now() + 30000;
  while (Date.now() < end) {
    primary.check();
    const value = read();
    if (value) return value;
    await delay(150);
  }
  throw Error("Release smoke readiness timed out");
}
const primary = launch();
let report;
// Keep rejection observed even if readiness times out.
void primary.done.catch(() => {});
try {
  report = await poll(() => {
    try {
      return JSON.parse(
        readFileSync(join(home, "config/smoke-ready.json"), "utf8"),
      );
    } catch {
      return null;
    }
  });
  if (!report.sqlite_open || !existsSync(join(home, "devone.db")))
    throw Error("SQLite/Home probe failed");
  const second = launch();
  await Promise.race([
    second.done,
    delay(10000).then(() => {
      second.child.kill();
      throw Error("Second instance did not activate and exit");
    }),
  ]);
  const session = JSON.parse(
    readFileSync(join(home, "config/desktop-session.json"), "utf8"),
  );
  if (session.pid !== primary.child.pid)
    throw Error("Second instance replaced controller");
  const folder = join(home, "www/watcher-probe");
  mkdirSync(folder);
  writeFileSync(join(folder, "index.html"), "<h1>Watcher probe</h1>");
  await poll(() => {
    try {
      return JSON.parse(
        readFileSync(join(home, "config/smoke-ready.json"), "utf8"),
      ).sites.includes("watcher-probe");
    } catch {
      return false;
    }
  });
  writeFileSync(join(home, "config/smoke-stop.txt"), report.shutdown_token);
  await primary.done;
  if (
    !JSON.parse(readFileSync(join(home, "config/smoke-stopped.json"), "utf8"))
      .clean_shutdown ||
    existsSync(join(home, "config/desktop-session.json"))
  )
    throw Error("Clean shutdown/activation cleanup failed");
  console.log(
    `PASS release executable: Home, SQLite, watcher discovery, second-instance activation, clean shutdown. Headless lifecycle only; no GUI acceptance. Evidence: ${home}`,
  );
} finally {
  if (primary.child.exitCode === null) {
    if (report)
      writeFileSync(join(home, "config/smoke-stop.txt"), report.shutdown_token);
    await Promise.race([primary.done.catch(() => {}), delay(5000)]);
    if (primary.child.exitCode === null) primary.child.kill();
  }
}
