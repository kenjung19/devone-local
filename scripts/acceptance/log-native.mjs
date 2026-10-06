// Keep interactive native acceptance output visible and preserve exact evidence.
import { spawn } from "node:child_process";
import { createWriteStream, mkdirSync } from "node:fs";
import { dirname, resolve } from "node:path";

const [file, command, ...args] = process.argv.slice(2);
if (!file || !command)
  throw Error("Supply evidence log, command and arguments");
const target = resolve(file);
mkdirSync(dirname(target), { recursive: true });
const log = createWriteStream(target);
let logFailed = false;
let approvalRequired = false;
let cancelled = false;
let skipped = false;
let captured = "";
log.on("error", (error) => {
  logFailed = true;
  console.error(`Acceptance evidence could not be written: ${error.message}`);
  // Let the native child complete its machine-resource cleanup; do not kill it.
});
const child = spawn(command, args, {
  shell: false,
  stdio: ["inherit", "pipe", "pipe"],
});
for (const [stream, output] of [
  [child.stdout, process.stdout],
  [child.stderr, process.stderr],
]) {
  stream.on("data", (chunk) => {
    captured = (captured + chunk.toString()).slice(-16384);
    approvalRequired ||= captured.includes("INTERACTIVE APPROVAL REQUIRED");
    cancelled ||= captured.includes("USER CANCELLED");
    skipped ||= captured.includes("SKIPPED - NOT ELEVATED");
    output.write(chunk);
    if (!logFailed) log.write(chunk);
  });
}
child.on("error", (error) => {
  console.error(`Native acceptance could not start: ${error.message}`);
});
process.on("SIGINT", () => {
  console.error(
    "Cancellation requested; waiting for native owned-resource cleanup. Dismiss the Windows confirmation if it is open.",
  );
});
child.on("close", (code) => {
  const failed = logFailed || code !== 0;
  const status = failed
    ? 1
    : approvalRequired
      ? 2
      : skipped
        ? 3
        : cancelled
          ? 4
          : 0;
  const result = failed
    ? "FAIL"
    : approvalRequired
      ? "INTERACTIVE APPROVAL REQUIRED"
      : skipped
        ? "SKIPPED - NOT ELEVATED"
        : cancelled
          ? "USER CANCELLED"
          : "PASS";
  console.log(`Optional machine certification: ${result}. Evidence: ${target}`);
  if (!logFailed) log.write(`\nOptional machine certification: ${result}\n`);
  if (logFailed) process.exitCode = status;
  else
    log.end(() => {
      process.exitCode = status;
    });
});
