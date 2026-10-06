import { describe, it, expect } from "vitest";
import {
  mkdtempSync,
  mkdirSync,
  readFileSync,
  rmSync,
  writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { createHash } from "node:crypto";
import {
  auditPayload,
  copyReleaseOutput,
  selectInstaller,
// @ts-expect-error Node-only release helper deliberately has no application types.
} from "../scripts/release-output.mjs";
describe("release artifact guard", () => {
  it("rejects missing or ambiguous installers before producing checksums", () => {
    const name = "DEVONE_0.1.0_x64-setup.exe";
    expect(
      selectInstaller([name, "DEVONE_0.2.0_x64-setup.exe"], "0.1.0"),
    ).toEqual([name]);
    expect(() => selectInstaller([], "0.1.0")).toThrow("found 0");
    expect(() =>
      selectInstaller([name, "Another_0.1.0_x64-setup.exe"], "0.1.0"),
    ).toThrow("found 2");
  });
  it("rejects prohibited and unknown payloads", () => {
    for (const path of [
      "tests",
      "fixtures",
      "project-staging",
      "DEVONE_HOME",
      "backups",
      "node_modules",
      "debug",
      "release-output",
    ])
      expect(() =>
        auditPayload(`File "\${MAINBINARYSRCPATH}"\nFile "${path}/extra.exe"`),
      ).toThrow();
    expect(auditPayload('File "${MAINBINARYSRCPATH}"').fixtureFeature).toBe(
      false,
    );
  });
  it("copies a checksummed installer and matching metadata without staging project data", () => {
    const root = mkdtempSync(join(tmpdir(), "devone-release-"));
    try {
      const source = join(root, "source");
      mkdirSync(source);
      const data = Buffer.from("installer-fixture");
      writeFileSync(join(source, "original.exe"), data);
      const report = [
        {
          filename: "original.exe",
          version: "0.1.0",
          bytes: data.length,
          sha256: createHash("sha256").update(data).digest("hex"),
        },
      ];
      const output = copyReleaseOutput(root, source, report);
      expect(
        readFileSync(join(output, "DEVONE-Local-0.1.0-Windows-x64-Setup.exe")),
      ).toEqual(data);
      expect(
        JSON.parse(
          readFileSync(join(output, "release-artifacts.json"), "utf8"),
        )[0].filename,
      ).toBe("DEVONE-Local-0.1.0-Windows-x64-Setup.exe");
      expect(() =>
        copyReleaseOutput(root, source, [
          { ...report[0], sha256: "0".repeat(64) },
        ]),
      ).toThrow("checksum mismatch");
      expect(() =>
        copyReleaseOutput(root, source, [
          { ...report[0], filename: "../outside.exe" },
        ]),
      ).toThrow();
    } finally {
      rmSync(root, { recursive: true, force: true });
    }
  });
});
