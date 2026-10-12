import { describe, it, expect } from "vitest";
import {
  auditPayload,
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
});
