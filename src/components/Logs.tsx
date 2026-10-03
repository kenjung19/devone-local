import { useEffect, useState } from "react";
import type { Site } from "../contracts";
import { bridge } from "../bridge";
export function Logs({ site }: { site?: Site }) {
  const [files, setFiles] = useState<string[]>([]);
  const [name, setName] = useState(
    site ? "site-" + site.id + ".log" : "devone.log",
  );
  const [text, setText] = useState("");
  const [error, setError] = useState("");
  useEffect(() => {
    bridge
      .logFiles()
      .then(setFiles)
      .catch((e) => setError(String(e)));
  }, []);
  useEffect(() => {
    if (!name) return;
    let alive = true;
    const load = () =>
      bridge
        .readLog(name)
        .then((t) => {
          if (alive) setText(t);
        })
        .catch((e) => {
          if (alive) setError(String(e));
        });
    void load();
    const id = setInterval(() => void load(), 4000);
    return () => {
      alive = false;
      clearInterval(id);
    };
  }, [name]);
  const visible = files.filter(
    (f) =>
      !site ||
      f === "devone.log" ||
      f === "caddy.log" ||
      f === "site-" + site.id + ".log" ||
      Object.entries(site.resolved).some(
        ([kind, version]) => f === kind + "-" + version + ".log",
      ),
  );
  const recent = text
    .split("\n")
    .filter((line) =>
      /error|warning|unexpected|fatal|failed|restart/i.test(line),
    )
    .slice(-12)
    .join("\n");
  return (
    <section className="panel">
      <div className="panel-header">
        <h2>{site ? site.hostname + " logs" : "Service logs"}</h2>
        <select value={name} onChange={(e) => setName(e.target.value)}>
          <option value="">Select log file</option>
          {visible.map((f) => (
            <option key={f}>{f}</option>
          ))}
        </select>
      </div>
      {error && <div className="alert error">{error}</div>}
      {recent && (
        <details open>
          <summary>Recent errors / warnings</summary>
          <pre className="log-output">{recent}</pre>
        </details>
      )}
      <pre className="log-output">
        {text || "Select a log file. The most recent 128 KiB will appear here."}
      </pre>
    </section>
  );
}
