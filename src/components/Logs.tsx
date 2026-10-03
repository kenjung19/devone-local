import { useEffect, useState } from "react";
import { bridge } from "../bridge";
export function Logs() {
  const [files, setFiles] = useState<string[]>([]);
  const [name, setName] = useState("");
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
  return (
    <section className="panel">
      <div className="panel-header">
        <h2>Service logs</h2>
        <select value={name} onChange={(e) => setName(e.target.value)}>
          <option value="">Select log file</option>
          {files.map((f) => (
            <option key={f}>{f}</option>
          ))}
        </select>
      </div>
      {error && <div className="alert error">{error}</div>}
      <pre className="log-output">
        {text || "Select a log file. The most recent 128 KiB will appear here."}
      </pre>
    </section>
  );
}
