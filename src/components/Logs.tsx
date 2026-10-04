import { useEffect, useState } from "react";
import type { Site } from "../contracts";
import { bridge } from "../bridge";
export function Logs({ site }: { site?: Site }) {
  const [files, setFiles] = useState<string[]>([]);
  const [name, setName] = useState(site ? "__combined__" : "devone.log");
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
      (name === "__combined__"
        ? bridge.logFiles().then(async (names) => {
            if (alive) setFiles(names);
            const relevant = names.filter((f) => siteLogMatches(f, site));
            return (
              await Promise.all(
                relevant.map(async (f) => `[${f}]\n${await bridge.readLog(f)}`),
              )
            ).join("\n\n");
          })
        : bridge.readLog(name)
      )
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
  }, [name, site]);
  const visible = files.filter((f) => siteLogMatches(f, site));
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
          {site && <option value="__combined__">All site streams</option>}
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

export function siteLogMatches(f: string, site?: Site): boolean {
  return (
    !site ||
    f === "devone.log" ||
    f === "caddy.log" ||
    f === `site-${site.id}.log` ||
    f.startsWith(`site-${site.id}-`) ||
    Object.entries(site.resolved).some(
      ([kind, version]) => f === `${kind}-${version}.log`,
    )
  );
}
