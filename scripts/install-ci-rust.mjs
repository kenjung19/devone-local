import { readFileSync } from "node:fs";
import { spawnSync } from "node:child_process";
const toolchain = readFileSync(
  new URL("../rust-toolchain.toml", import.meta.url),
  "utf8",
);
const channel = toolchain.match(/^channel\s*=\s*"([^"]+)"/m)?.[1];
const profile = toolchain.match(/^profile\s*=\s*"([^"]+)"/m)?.[1];
const components = [
  ...(toolchain.match(/^components\s*=\s*\[([^\]]*)\]/m)?.[1] ?? "").matchAll(
    /"([^"]+)"/g,
  ),
].map((m) => m[1]);
if (!channel || !profile || !components.length)
  throw Error("Invalid pinned Rust toolchain");
const result = spawnSync(
  "rustup",
  [
    "toolchain",
    "install",
    "--no-self-update",
    channel,
    "--profile",
    profile,
    "--component",
    components.join(","),
  ],
  { stdio: "inherit", shell: false },
);
if (result.error) throw result.error;
if (result.status !== 0) process.exit(result.status ?? 1);
