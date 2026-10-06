import { ErrorNotice } from "./ErrorNotice";
import { useState } from "react";
import type {
  Snapshot,
  Action,
  RuntimeKind,
  InstallProgress,
} from "../contracts";
import { SetupRuntime } from "./SetupRuntime";
import { CaUpgradeNotice } from "./CaUpgradeNotice";
const steps = [
  "Home",
  "Web Server",
  "PHP",
  "MySQL",
  "Local Domains",
  "HTTPS",
  "พร้อมเริ่มงาน",
];
export function Setup({
  data,
  busy,
  error,
  progress,
  act,
  importRuntime,
  close,
  initialStep = 0,
}: {
  data: Snapshot;
  busy: boolean;
  error: string;
  progress: InstallProgress | null;
  act: (a: Action) => Promise<boolean>;
  importRuntime: (k: RuntimeKind) => void;
  close: () => void;
  initialStep?: number;
}) {
  const [step, setStep] = useState(initialStep);
  const s = data.setup;
  const ready = [
    s.home_ready,
    s.caddy,
    s.php,
    s.mysql,
    s.dns_policy && s.dns_server && s.dns_system,
    s.ca_present && s.ca_trusted,
  ];
  const allReady = ready.every(Boolean);
  const runtime =
    step >= 1 && step <= 3
      ? (["caddy", "php", "mysql"] as const)[step - 1]
      : null;
  return (
    <section className="setup-page" aria-label="ตั้งค่า DEVONE Local">
      <header className="setup-header">
        <div className="brand">
          <span className="brand-mark">D</span>
          <div>
            DEVONE<span>LOCAL DEVELOPMENT</span>
          </div>
        </div>
        <button disabled={busy} onClick={close}>
          ตั้งค่าภายหลัง
        </button>
      </header>
      <div className="setup-layout">
        <div className="setup-intro">
          <p className="eyebrow">เริ่มต้นใช้งาน</p>
          <h1>ตั้งค่า DEVONE Local</h1>
          <p>เตรียมพื้นที่ทำงานทีละขั้น กลับมาแก้ไขได้จาก Settings</p>
        </div>
        <nav className="setup-steps" aria-label="ขั้นตอนการตั้งค่า">
          {steps.map((label, index) => (
            <button
              key={label}
              disabled={busy}
              aria-current={index === step ? "step" : undefined}
              className={index === step ? "setup-step current" : "setup-step"}
              onClick={() => setStep(index)}
            >
              <span
                className={ready[index] ? "step-number ready" : "step-number"}
              >
                {ready[index] ? "✓" : index + 1}
              </span>
              {label}
            </button>
          ))}
        </nav>
        <div className="setup-stage" aria-live="polite">
          <p className="setup-position">
            ขั้นตอน {step + 1} จาก {steps.length}
          </p>
          {step === 0 && (
            <>
              <h2>พื้นที่สำหรับโปรเจกต์ของคุณ</h2>
              <p>
                DEVONE เก็บโปรเจกต์ runtimes ฐานข้อมูล และการตั้งค่าไว้ใน Home
                นี้
              </p>
              <div className="setup-home">
                <code>{data.home}</code>
                <span>{s.home_ready ? "พร้อมใช้งาน" : "ยังไม่พร้อม"}</span>
              </div>
              <p>
                วางโฟลเดอร์โปรเจกต์ใน www แล้ว DEVONE
                จะค้นพบโปรเจกต์ให้อัตโนมัติ
              </p>
              <button
                disabled={busy}
                onClick={() => void act({ type: "open_folder", site_id: null })}
              >
                เปิด www
              </button>
            </>
          )}
          {runtime && (
            <>
              <h2>
                {runtime === "caddy"
                  ? "เตรียมเว็บเซิร์ฟเวอร์"
                  : runtime === "php"
                    ? "เลือก PHP เริ่มต้น"
                    : "เลือกฐานข้อมูล MySQL"}
              </h2>
              <SetupRuntime
                key={runtime}
                data={data}
                kind={runtime}
                busy={busy}
                act={act}
                importRuntime={importRuntime}
              />
            </>
          )}
          {step === 4 && (
            <>
              <h2>เปิดโปรเจกต์ด้วยชื่อ .test</h2>
              <p>
                เช่น demo.test จะชี้ไปที่เครื่องนี้ Windows จะขอสิทธิ์ UAC
                เฉพาะตอนตั้งค่า DNS ส่วนตัวแอปยังทำงานด้วยสิทธิ์ผู้ใช้ปกติ
              </p>
              <dl className="setup-checks">
                <div>
                  <dt>Automatic .test domains</dt>
                  <dd>{s.dns_policy ? "ติดตั้งแล้ว" : "ต้องตั้งค่า"}</dd>
                </div>
                <div>
                  <dt>Local domain service</dt>
                  <dd>{s.dns_server ? "ทำงานอยู่" : "ตรวจพอร์ต UDP/TCP 53"}</dd>
                </div>
                <div>
                  <dt>Domain availability</dt>
                  <dd>
                    {s.dns_system && s.dns_server
                      ? "พร้อมใช้งาน"
                      : "กำลังตรวจ / ยังไม่พร้อม"}
                  </dd>
                </div>
              </dl>
              <button disabled={busy} onClick={() => void act({ type: "dns" })}>
                ตั้งค่า / Retry DNS
              </button>
            </>
          )}
          {step === 5 && (
            <>
              <h2>ใช้งาน HTTPS ในเครื่อง</h2>
              <CaUpgradeNotice data={data} busy={busy} act={act} />
              <p>
                สร้าง Local CA แล้วติดตั้ง trust ในบัญชี Windows นี้
                เพื่อเปิดโปรเจกต์ผ่าน HTTPS
              </p>
              <dl className="setup-checks">
                <div>
                  <dt>Local CA</dt>
                  <dd>{s.ca_present ? "สร้างแล้ว" : "ยังไม่ได้สร้าง"}</dd>
                </div>
                <div>
                  <dt>Windows trust</dt>
                  <dd>{s.ca_trusted ? "เชื่อถือแล้ว" : "ยังไม่ได้ติดตั้ง"}</dd>
                </div>
              </dl>
              {!s.caddy && <p>ติดตั้ง Caddy ในขั้นตอนที่ 2 ก่อนสร้าง CA</p>}
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
            </>
          )}
          {step === 6 && (
            <>
              <h2>
                {allReady ? "พร้อมเริ่มทำงานแล้ว" : "ตรวจความพร้อมก่อนเริ่ม"}
              </h2>
              <dl className="setup-checks setup-summary">
                {steps.slice(0, 6).map((label, index) => (
                  <div key={label}>
                    <dt>{label}</dt>
                    <dd>{ready[index] ? "พร้อม" : "ยังไม่พร้อม"}</dd>
                  </div>
                ))}
              </dl>
              <p>
                {allReady
                  ? "วางโปรเจกต์ใน www แล้วเปิดจากหน้า Sites ได้เลย"
                  : "เลือกขั้นตอนด้านบนเพื่อแก้ไข หรือข้ามส่วนที่ยังไม่พร้อมแล้วกลับมาตั้งค่าภายหลัง"}
              </p>
            </>
          )}
          {busy && progress && (
            <p className="setup-progress" role="status">
              {progress.runtime} · {progress.phase} ·{" "}
              {progress.bytes.toLocaleString()} bytes{" "}
              {progress.total ? "/ " + progress.total.toLocaleString() : ""}
            </p>
          )}
          <ErrorNotice error={error} />
        </div>
        <footer className="setup-controls">
          <button
            disabled={busy || step === 0}
            onClick={() => setStep(step - 1)}
          >
            ย้อนกลับ
          </button>
          <div className="actions">
            {step < 6 ? (
              <button
                className="primary"
                disabled={busy}
                onClick={() => setStep(step + 1)}
              >
                ถัดไป
              </button>
            ) : (
              <>
                <button
                  disabled={busy}
                  onClick={() => void act({ type: "finish_setup", skip: true })}
                >
                  ข้ามส่วนที่ยังไม่พร้อม
                </button>
                <button
                  className="primary"
                  disabled={busy || !allReady}
                  onClick={() =>
                    void act({ type: "finish_setup", skip: false })
                  }
                >
                  เสร็จสิ้นและเปิด environment
                </button>
              </>
            )}
          </div>
        </footer>
      </div>
    </section>
  );
}
