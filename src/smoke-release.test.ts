import { describe, it, expect } from "vitest";
import { fileURLToPath } from "node:url";
import {
  smokeExecutable,
  assertSmokeAlive,
// @ts-expect-error Node-only helper deliberately has no application types.
} from "../scripts/smoke-support.mjs";
describe("release smoke startup", () => {
  it("resolves the default executable relative to the repository script", () => {
    expect(smokeExecutable()).toBe(
      fileURLToPath(
        new URL(
          "../src-tauri/target/x86_64-pc-windows-msvc/release/devone-local.exe",
          import.meta.url,
        ),
      ),
    );
  });
  it("reports early successful/nonzero/signal exits and spawn failures", () => {
    for (const code of [0, 7])
      expect(() =>
        assertSmokeAlive({ exitCode: code, signalCode: null }),
      ).toThrow(`before readiness: ${code}`);
    expect(() =>
      assertSmokeAlive({ exitCode: null, signalCode: "SIGTERM" }),
    ).toThrow("SIGTERM");
    expect(() =>
      assertSmokeAlive(
        { exitCode: null, signalCode: null },
        Error("spawn failed"),
      ),
    ).toThrow("spawn failed");
    expect(() =>
      assertSmokeAlive({ exitCode: null, signalCode: null }),
    ).not.toThrow();
  });
});
