export function selectInstaller(filenames, version) {
  const matches = filenames.filter(
    (name) => name.includes(`_${version}_x64`) && name.endsWith("-setup.exe"),
  );
  if (matches.length !== 1)
    throw Error(
      `Expected exactly one ${version} x64 NSIS installer; found ${matches.length}`,
    );
  return matches;
}
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
