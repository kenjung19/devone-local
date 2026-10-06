import { fileURLToPath } from "node:url";
import { resolve } from "node:path";
export function smokeExecutable(argument) {
  return argument
    ? resolve(argument)
    : fileURLToPath(
        new URL(
          "../src-tauri/target/x86_64-pc-windows-msvc/release/devone-local.exe",
          import.meta.url,
        ),
      );
}
export function assertSmokeAlive(child, error) {
  if (error) throw error;
  if (child.exitCode !== null || child.signalCode !== null)
    throw Error(
      `Executable exited before readiness: ${child.exitCode ?? child.signalCode}`,
    );
}
