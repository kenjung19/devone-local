import type {
  Snapshot,
  Action,
  RuntimeKind,
  InstallProgress,
} from "../contracts";
export function Setup({
  data,
  busy,
  error,
  progress,
  act,
  importRuntime,
  close,
}: {
  data: Snapshot;
  busy: boolean;
  error: string;
  progress: InstallProgress | null;
  act: (a: Action) => Promise<void>;
  importRuntime: (k: RuntimeKind) => void;
  close: () => void;
}) {
  const s = data.setup;
  return (
    <div className="overlay">
      <section className="dialog setup-dialog">
        <h2>ตั้งค่า DEVONE Local</h2>
        {busy && progress && (
          <p role="status">
            {progress.runtime} · {progress.phase} ·{" "}
            {progress.bytes.toLocaleString()} bytes{" "}
            {progress.total ? "/ " + progress.total.toLocaleString() : ""}
          </p>
        )}
        <p>
          ตั้งค่าครั้งเดียว แอปทำงานด้วยสิทธิ์ผู้ใช้ปกติ เปิดกลับมาตรวจหรือ
          retry ได้จาก Settings
        </p>
        <div className="settings-row">
          <div>
            <strong>1. DEVONE Home</strong>
            <p>
              <code>{data.home}</code> ·{" "}
              {s.home_ready ? "พร้อมใช้งาน" : "ยังไม่พร้อม"}
            </p>
          </div>
          <button
            onClick={() => void act({ type: "open_folder", site_id: null })}
          >
            เปิด www
          </button>
        </div>
        <div className="settings-row">
          <div>
            <strong>2. Wildcard DNS *.test</strong>
            <p>
              System lookup:{" "}
              {s.dns_system ? "พร้อม" : "กำลังตรวจ / ยังไม่พร้อม"} · Policy:{" "}
              {s.dns_policy ? "พร้อม" : "ต้องตั้งค่า"} · Resolver:{" "}
              {s.dns_server ? "พร้อม" : "ตรวจพอร์ต UDP/TCP 53"}
              <br />
              ตอบเฉพาะ .test → 127.0.0.1 การตั้งค่า Windows แสดง UAC ครั้งเดียว
            </p>
          </div>
          <button disabled={busy} onClick={() => void act({ type: "dns" })}>
            ตั้งค่า / Retry DNS
          </button>
        </div>
        {(["caddy", "php", "mysql"] as const).map((kind) => (
          <div className="settings-row" key={kind}>
            <div>
              <strong>{kind.toUpperCase()}</strong>
              {kind === "mysql" &&
                s.mysql &&
                !data.databases.some(
                  (d) => d.runtime_id === "mysql:" + data.defaults.mysql,
                ) && (
                  <button
                    disabled={busy}
                    onClick={() =>
                      void act({
                        type: "database",
                        runtime: {
                          kind: "mysql",
                          version: data.defaults.mysql,
                        },
                        operation: "initialize",
                      })
                    }
                  >
                    Initialize MySQL instance
                  </button>
                )}
              <p>
                {s[kind]
                  ? "ติดตั้งแล้ว"
                  : "เลือกเวอร์ชันหรือนำเข้า runtime ที่มีอยู่"}
              </p>
              <div className="actions">
                {data.available
                  .filter(
                    (m) =>
                      m.platform === data.platform &&
                      m.runtime === kind &&
                      !data.installed.some(
                        (r) => r.id === kind + ":" + m.version,
                      ),
                  )
                  .map((m) => (
                    <button
                      key={m.version}
                      disabled={busy || !m.sha256 || !m.download}
                      onClick={() =>
                        void act({
                          type: "install",
                          runtime: { kind, version: m.version },
                        })
                      }
                    >
                      ติดตั้ง {m.version}
                    </button>
                  ))}
              </div>
            </div>
            <button disabled={busy} onClick={() => importRuntime(kind)}>
              Import
            </button>
          </div>
        ))}
        <div className="settings-row">
          <div>
            <strong>3. HTTPS และ Local CA</strong>
            <p>
              {s.ca_present ? "สร้าง CA แล้ว" : "ยังไม่ได้สร้าง CA"} ·{" "}
              {s.ca_trusted
                ? "Windows เชื่อถือแล้ว"
                : "ยังไม่มี trust ในบัญชีผู้ใช้นี้"}
            </p>
          </div>
          <div className="actions">
            <button
              disabled={busy || !s.caddy}
              onClick={() => void act({ type: "prepare_ca" })}
            >
              สร้าง / ตรวจ CA
            </button>
            <button
              disabled={busy || !s.ca_present}
              onClick={() => void act({ type: "trust" })}
            >
              ติดตั้ง trust
            </button>
          </div>
        </div>
        {error && (
          <p className="error" role="alert">
            {error}
          </p>
        )}
        <div className="actions">
          <button disabled={busy} onClick={close}>
            จัดการ runtimes / Settings
          </button>
          <button
            disabled={busy}
            onClick={() => void act({ type: "finish_setup", skip: true })}
          >
            ข้ามส่วนที่ยังไม่พร้อม
          </button>
          <button
            className="primary"
            disabled={busy}
            onClick={() => void act({ type: "finish_setup", skip: false })}
          >
            เสร็จสิ้นและเปิด environment
          </button>
        </div>
      </section>
    </div>
  );
}
