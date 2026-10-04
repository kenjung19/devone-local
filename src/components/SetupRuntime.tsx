import { useState } from "react";
import type { Snapshot, Action, RuntimeKind } from "../contracts";
export function SetupRuntime({
  data,
  kind,
  busy,
  act,
  importRuntime,
}: {
  data: Snapshot;
  kind: Exclude<RuntimeKind, "node">;
  busy: boolean;
  act: (a: Action) => Promise<void>;
  importRuntime: (k: RuntimeKind) => void;
}) {
  const versions = Array.from(
    new Set([
      ...data.installed
        .filter((r) => r.manifest.runtime === kind)
        .map((r) => r.manifest.version),
      ...data.available
        .filter(
          (m) =>
            m.runtime === kind &&
            m.platform === data.platform &&
            m.download &&
            m.sha256,
        )
        .map((m) => m.version),
    ]),
  ).sort((a, b) => b.localeCompare(a, undefined, { numeric: true }));
  const [selected, setSelected] = useState(
    data.defaults[kind] ?? versions[0] ?? "",
  );
  const installed = data.installed.some(
    (r) => r.manifest.runtime === kind && r.manifest.version === selected,
  );
  return (
    <div className="settings-row">
      <div>
        <strong>{kind.toUpperCase()}</strong>
        <p>
          {data.setup[kind]
            ? "ติดตั้งแล้ว"
            : "เลือกเวอร์ชันเริ่มต้นเฉพาะรุ่นที่ต้องการ หรือนำเข้า runtime ที่มีอยู่"}
        </p>
        {kind === "caddy" && (
          <p>Caddy จัดการเว็บและ HTTPS ให้ DEVONE ติดตั้งได้จากปุ่มนี้</p>
        )}
        <div className="actions">
          <select
            aria-label={`Initial ${kind.toUpperCase()} version`}
            disabled={busy || versions.length === 0}
            value={selected}
            onChange={(e) => setSelected(e.target.value)}
          >
            {versions.length === 0 && (
              <option value="">Import a custom runtime</option>
            )}
            {versions.map((v) => (
              <option key={v} value={v}>
                {v}
                {data.installed.some(
                  (r) =>
                    r.manifest.runtime === kind && r.manifest.version === v,
                )
                  ? " · Installed"
                  : ""}
              </option>
            ))}
          </select>
          <button
            disabled={
              busy ||
              !selected ||
              (installed && data.defaults[kind] === selected)
            }
            onClick={() =>
              void act({
                type: installed ? "default" : "install",
                runtime: { kind, version: selected },
              })
            }
          >
            {installed
              ? "ใช้เวอร์ชันนี้เป็น default"
              : "ติดตั้งเวอร์ชันที่เลือก"}
          </button>
        </div>
        {kind === "mysql" &&
          data.setup.mysql &&
          !data.databases.some(
            (d) =>
              d.runtime_id === "mysql:" + data.defaults.mysql && d.initialized,
          ) && (
            <button
              disabled={busy}
              onClick={() =>
                void act({
                  type: "database",
                  runtime: { kind: "mysql", version: data.defaults.mysql },
                  operation: "initialize",
                })
              }
            >
              Initialize MySQL instance
            </button>
          )}
      </div>
      <button disabled={busy} onClick={() => importRuntime(kind)}>
        Import custom version
      </button>
    </div>
  );
}
