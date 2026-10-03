import { readFileSync } from "node:fs";
import { describe, it, expect } from "vitest";
describe("locked development toolchain", () => {
  it("uses package application version as the release source without drifting Rust metadata", () => {
    const pkg = JSON.parse(readFileSync("package.json", "utf8"));
    const config = JSON.parse(
      readFileSync("src-tauri/tauri.conf.json", "utf8"),
    );
    const cargo = readFileSync("src-tauri/Cargo.toml", "utf8");
    expect(config.version).toBe("../package.json");
    expect(
      cargo.match(/\[package\][\s\S]*?\nversion\s*=\s*"([^"]+)"/)?.[1],
    ).toBe(pkg.version);
    expect(config.bundle.windows.nsis.installMode).toBe("currentUser");
    expect(config.bundle.targets).toEqual(["nsis"]);
    expect(pkg.scripts["release:windows"]).toBe(
      "node scripts/release-windows.mjs",
    );
  });
  it("keeps direct frontend dependencies exact and compiler/API aliases explicit", () => {
    const pkg = JSON.parse(readFileSync("package.json", "utf8")) as {
      packageManager: string;
      engines: { node: string; pnpm: string };
      dependencies: Record<string, string>;
      devDependencies: Record<string, string>;
    };
    expect(pkg.packageManager).toBe("pnpm@" + pkg.engines.pnpm);
    expect(readFileSync(".node-version", "utf8").trim()).toBe(pkg.engines.node);
    for (const version of Object.values({
      ...pkg.dependencies,
      ...pkg.devDependencies,
    })) {
      expect(version).toMatch(/^(?:npm:.+@)?\d+\.\d+\.\d+$/);
    }
    expect(pkg.devDependencies["@typescript/native"]).toBe(
      "npm:typescript@7.0.2",
    );
    expect(pkg.devDependencies.typescript).toBe(
      "npm:@typescript/typescript6@6.0.2",
    );
  });
  it("pins Rust direct crates and keeps source catalog data outside runtime code", () => {
    const cargo = readFileSync("src-tauri/Cargo.toml", "utf8");
    for (const line of cargo.split("\n")) {
      if (
        /^(tauri(?:-build)?|serde(?:_json)?|rusqlite|thiserror|notify|ctrlc|tracing(?:-subscriber)?|uuid|sha2|reqwest|zip|windows-sys|tempfile|winreg|mysql|zeroize)\s*=/.test(
          line,
        )
      )
        expect(line).toMatch(/(?:version\s*=\s*)?"=\d+\.\d+\.\d+"/);
    }
    expect(readFileSync("rust-toolchain.toml", "utf8")).toContain(
      'channel = "1.99.0"',
    );
    const catalog = JSON.parse(
      readFileSync("src-tauri/assets/runtime-catalog.json", "utf8"),
    ) as {
      schema_version: number;
      manifests: { download: string; sha256: string }[];
    };
    expect(catalog.schema_version).toBe(1);
    for (const manifest of catalog.manifests) {
      expect(manifest.download).toMatch(/^https:\/\//);
      expect(manifest.sha256).toMatch(/^[0-9a-f]{64}$/);
    }
  });
});
