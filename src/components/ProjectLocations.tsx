import { useState } from "react";
import { bridge } from "../bridge";
import type { Action, Snapshot } from "../contracts";

export function ProjectLocations({
  data,
  busy,
  act,
}: {
  data: Snapshot;
  busy: boolean;
  act: (action: Action) => Promise<boolean>;
}) {
  const [root, setRoot] = useState("");
  const [external, setExternal] = useState("");
  const [error, setError] = useState("");
  const disabled =
    busy ||
    data.active ||
    data.services.some(
      (s) => s.key.startsWith("site:") && s.status === "running",
    ) ||
    data.dependency_tasks?.some((t) => t.status === "running") ||
    data.developer?.creation_tasks.some((t) => t.status === "running");
  const choose = async (set: (value: string) => void) => {
    try {
      setError("");
      const path = await bridge.selectProjectFolder();
      if (path) set(path);
    } catch (e) {
      setError(String(e));
    }
  };
  return (
    <div className="panel-copy project-locations">
      <p>
        เลือกโฟลเดอร์โปรเจกต์หลัก หรือเพิ่มโปรเจกต์จากโฟลเดอร์อื่นได้
        ไฟล์จะไม่ถูกย้าย Runtime และฐานข้อมูลยังอยู่ใน Home เดิม
      </p>
      <p>
        ถ้าจะย้ายโปรเจกต์ไปโฟลเดอร์ใหม่ ให้ย้ายโฟลเดอร์โปรเจกต์ไปก่อน
        แล้วค่อยเปลี่ยน Projects folder โปรเจกต์ที่ชื่อโฟลเดอร์เดิมจะใช้การตั้งค่า
        การผูกฐานข้อมูล และ process เดิมต่อ
      </p>
      {data.active && <p>หยุด environment ก่อนเปลี่ยนโฟลเดอร์โปรเจกต์</p>}
      <div className="creation-fields">
        <label>
          Projects folder (www)
          <input
            value={root}
            placeholder={data.projects_root ?? `${data.home}/www`}
            disabled={disabled}
            onChange={(e) => setRoot(e.target.value)}
          />
        </label>
        <div className="actions">
          <button disabled={disabled} onClick={() => void choose(setRoot)}>
            เลือก Projects folder
          </button>
          <button
            disabled={disabled || !root.trim()}
            onClick={() =>
              void act({ type: "projects_root", path: root.trim() }).then(
                (ok) => {
                  if (ok) setRoot("");
                },
              )
            }
          >
            ใช้โฟลเดอร์นี้
          </button>
          <button
            disabled={
              disabled ||
              !data.projects_root ||
              data.projects_root.replaceAll("\\", "/") ===
                `${data.home.replaceAll("\\", "/")}/www`
            }
            onClick={() =>
              void act({ type: "projects_root", path: null }).then((ok) => {
                if (ok) setRoot("");
              })
            }
          >
            กลับไปใช้ Home/www
          </button>
        </div>
        <label>
          เพิ่มโปรเจกต์จากโฟลเดอร์อื่น
          <input
            value={external}
            placeholder="E:\Projects\my-app"
            disabled={disabled}
            onChange={(e) => setExternal(e.target.value)}
          />
        </label>
        <div className="actions">
          <button disabled={disabled} onClick={() => void choose(setExternal)}>
            เลือกโฟลเดอร์โปรเจกต์
          </button>
          <button
            disabled={disabled || !external.trim()}
            onClick={() =>
              void act({
                type: "external_project",
                path: external.trim(),
                remove: false,
              }).then((ok) => {
                if (ok) setExternal("");
              })
            }
          >
            เพิ่มโปรเจกต์
          </button>
        </div>
      </div>
      {(data.external_projects ?? []).map((path) => (
        <div className="external-project" key={path}>
          <code>{path}</code>
          <button
            disabled={disabled}
            onClick={() =>
              void act({ type: "external_project", path, remove: true })
            }
          >
            เอาออกจากรายการ
          </button>
        </div>
      ))}
      {(data.external_projects?.length ?? 0) > 0 && (
        <p>
          เอาออกจากรายการจะหยุดติดตามโฟลเดอร์เท่านั้น ไม่ลบโปรเจกต์หรือฐานข้อมูล
        </p>
      )}
      {error && <p role="alert">{error}</p>}
    </div>
  );
}
