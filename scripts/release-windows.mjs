import { spawnSync } from "node:child_process";
import { readdirSync, readFileSync, writeFileSync } from "node:fs";
import { createHash } from "node:crypto";
import { join } from "node:path";
import { checkVersion } from "./check-version.mjs";
if (process.platform !== "win32")
  throw new Error(
    "Windows release packaging requires Windows with MSVC Build Tools.",
  );
const { version, root } = checkVersion();
const target = "x86_64-pc-windows-msvc";
const cli = join(root, "node_modules/@tauri-apps/cli/tauri.js");
const result = spawnSync(
  process.execPath,
  [cli, "build", "--target", target, "--bundles", "nsis"],
  { cwd: root, stdio: "inherit", shell: false },
);
if (result.error) throw result.error;
if (result.status !== 0) process.exit(result.status ?? 1);
const directory = join(root, "src-tauri/target", target, "release/bundle/nsis");
const artifacts = readdirSync(directory).filter(
  (name) => name.includes(`_${version}_x64`) && name.endsWith("-setup.exe"),
);
if (!artifacts.length)
  throw new Error(`No ${version} x64 NSIS installer found in ${directory}`);
const report = artifacts.map((filename) => {
  const bytes = readFileSync(join(directory, filename));
  return {
    filename,
    version,
    target,
    installer: "NSIS current-user",
    bytes: bytes.length,
    sha256: createHash("sha256").update(bytes).digest("hex"),
  };
});
writeFileSync(
  join(directory, "release-artifacts.json"),
  JSON.stringify(report, null, 2) + "\n",
);
console.log(`Release artifacts: ${directory}`);
for (const artifact of report)
  console.log(
    `${artifact.filename} (${artifact.bytes} bytes) SHA-256 ${artifact.sha256}`,
  );
