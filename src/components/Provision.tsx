import { bridge } from "../bridge";
import { useState } from "react";
import type { Action, Snapshot, Site } from "../contracts";
export function Provision({
  site,
  data,
  busy,
  act,
}: {
  site: Site;
  data: Snapshot;
  busy: boolean;
  act: (a: Action) => Promise<void>;
}) {
  const binding = data.project_databases.find((b) => b.site_id === site.id);
  const [name, setName] = useState(
    binding?.database_name ??
      site.name
        .toLowerCase()
        .replace(/[^a-z0-9_]/g, "_")
        .slice(0, 63),
  );
  const [revealed, setRevealed] = useState("");
  const [error, setError] = useState("");
  const port = data.databases.find(
    (d) => d.runtime_id === binding?.runtime_id,
  )?.port;
  return (
    <section className="panel">
      <div className="panel-header">
        <h2>Project database</h2>
      </div>
      <div className="panel-copy">
        <p>
          สร้าง database และ user ที่มีสิทธิ์เฉพาะ database นี้
          เก็บรหัสผ่านเข้ารหัสสำหรับบัญชี Windows นี้ ไฟล์ .env
          ของโปรเจกต์จะคงเดิม
        </p>
        <label>
          Database name
          <input
            value={name}
            disabled={busy || !!binding}
            onChange={(e) => setName(e.target.value)}
            pattern="[A-Za-z0-9_]{1,63}"
          />
        </label>
        <button
          disabled={busy || !site.resolved.mysql || !name}
          onClick={() =>
            void act({
              type: "provision",
              site_id: site.id,
              database_name: name,
            })
          }
        >
          {binding ? "ตรวจ / Retry provisioning" : "สร้าง database และ user"}
        </button>
        {binding && (
          <>
            <dl>
              <dt>Instance</dt>
              <dd>
                {binding.runtime_id} · {binding.status}
              </dd>
              <dt>Host / Port</dt>
              <dd>127.0.0.1:{port ?? "stopped"}</dd>
              <dt>User</dt>
              <dd>{binding.username}</dd>
              <dt>Credential reference</dt>
              <dd>{binding.credential_ref}</dd>
            </dl>
            <button
              onClick={() => {
                void (async () => {
                  try {
                    const r = await bridge.execute({
                      type: "reveal_credential",
                      site_id: site.id,
                    });
                    setRevealed(r.message ?? "");
                    setError("");
                  } catch (e) {
                    setError(String(e));
                  }
                })();
              }}
            >
              แสดงรหัสผ่าน
            </button>
            {revealed && (
              <p>
                <code>{revealed}</code>{" "}
                <button onClick={() => setRevealed("")}>ซ่อน</button>
              </p>
            )}
          </>
        )}
        {error && <p className="error">{error}</p>}
      </div>
    </section>
  );
}
