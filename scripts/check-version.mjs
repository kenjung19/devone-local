import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
const root = new URL("../", import.meta.url);
export function checkVersion() {
  const pkg = JSON.parse(readFileSync(new URL("package.json", root), "utf8"));
  const config = JSON.parse(
    readFileSync(new URL("src-tauri/tauri.conf.json", root), "utf8"),
  );
  const cargo = readFileSync(new URL("src-tauri/Cargo.toml", root), "utf8");
  const rustVersion = cargo.match(
    /\[package\][\s\S]*?\nversion\s*=\s*"([^"]+)"/,
  )?.[1];
  if (
    !/^\d+\.\d+\.\d+$/.test(pkg.version) ||
    config.version !== "../package.json" ||
    rustVersion !== pkg.version
  )
    throw new Error(
      "Version mismatch: package.json is authoritative; synchronize Cargo package version explicitly.",
    );
  return { version: pkg.version, root: fileURLToPath(root) };
}
if (import.meta.main)
  console.log(
    `DEVONE Local ${checkVersion().version}: version metadata consistent`,
  );
