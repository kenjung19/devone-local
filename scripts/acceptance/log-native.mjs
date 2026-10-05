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
    output.write(chunk);
    if (!logFailed) log.write(chunk);
  });
}
child.on("error", (error) => {
  console.error(`Native acceptance could not start: ${error.message}`);
});
child.on("close", (code) => {
  const status = logFailed || code == null || code < 0 ? 1 : code;
  if (logFailed) process.exitCode = status;
  else
    log.end(() => {
      process.exitCode = status;
    });
});
