import {
  copyFileSync,
  lstatSync,
  mkdirSync,
  readFileSync,
  writeFileSync,
} from "node:fs";
import { join, basename } from "node:path";
import { createHash } from "node:crypto";
export function auditPayload(script) {
  const statements = script
    .split(/\r?\n/)
    .filter((line) => /^\s*File\s/i.test(line));
  const allowed =
    /\$\{MAINBINARYSRCPATH\}|\$\{WEBVIEW2(?:BOOTSTRAPPER|INSTALLER)PATH\}|release[\\/]devone-core\.exe/i;
  for (const line of statements)
    if (
      !allowed.test(line) ||
      /tests|fixtures|temporary.projects|project-staging|backups|node_modules|DEVONE_HOME|[\\/]debug[\\/]|devone-process-fixture/i.test(
        line,
      )
    )
      throw Error(`Unexpected installer payload: ${line.trim()}`);
  if (!statements.some((line) => line.includes("${MAINBINARYSRCPATH}")))
    throw Error("Product executable missing from installer payload");
  return {
    payloadStatements: statements.map((line) => line.trim()),
    fixtureFeature: false,
    projectDependenciesBundled: false,
  };
}
function rejectLink(path) {
  try {
    if (lstatSync(path).isSymbolicLink())
      throw Error(`Release output must not be a link: ${path}`);
  } catch (error) {
    if (error.code !== "ENOENT") throw error;
  }
}
export function copyReleaseOutput(root, source, artifacts) {
  const output = join(root, "release");
  rejectLink(output);
  mkdirSync(output, { recursive: true });
  const report = artifacts.map((artifact) => {
    if (
      basename(artifact.filename) !== artifact.filename ||
      !/^\d+\.\d+\.\d+$/.test(artifact.version)
    )
      throw Error("Invalid release artifact name/version");
    const filename = `DEVONE-Local-${artifact.version}-Windows-x64-Setup.exe`;
    const destination = join(output, filename);
    rejectLink(destination);
    copyFileSync(join(source, artifact.filename), destination);
    const bytes = readFileSync(destination);
    if (
      bytes.length !== artifact.bytes ||
      createHash("sha256").update(bytes).digest("hex") !== artifact.sha256
    )
      throw Error("Copied installer checksum mismatch");
    return { ...artifact, filename, originalFilename: artifact.filename };
  });
  const metadata = join(output, "release-artifacts.json");
  rejectLink(metadata);
  writeFileSync(metadata, JSON.stringify(report, null, 2) + "\n");
  return output;
}
