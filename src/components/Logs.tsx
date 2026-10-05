import { useEffect, useRef, useState } from "react";
import type { Site } from "../contracts";
import { bridge } from "../bridge";
import { ErrorNotice } from "./ErrorNotice";
export function logLabel(name: string) {
  if (/creation/i.test(name)) return "Project creation";
  if (/dependencies|install/i.test(name)) return "Dependency install";
  if (/mysql/i.test(name)) return "MySQL";
  if (/php/i.test(name)) return "PHP";
  if (/vite/i.test(name)) return "Vite";
  if (/queue|scheduler/i.test(name)) return "Queue / Scheduler";
  if (/caddy/i.test(name)) return "Web";
  if (/mailpit/i.test(name)) return "Mailpit";
  if (/site-.*web/i.test(name)) return "Node / Web";
  return name === "devone.log" ? "DEVONE Local" : "Project process";
}
export function Logs({ site }: { site?: Site }) {
  const [files, setFiles] = useState<string[]>([]),
    [name, setName] = useState("");
  const [text, setText] = useState(""),
    [error, setError] = useState("");
  const output = useRef<HTMLPreElement>(null);
  useEffect(() => {
    let alive = true;
    const load = () =>
      void bridge
        .logFiles()
        .then((f) => {
          if (alive) setFiles(f);
        })
        .catch((e) => {
          if (alive) setError(String(e));
        });
    load();
    const timer = setInterval(load, 4000);
    return () => {
      alive = false;
      clearInterval(timer);
    };
  }, []);
  useEffect(() => {
    if (!name) return;
    let alive = true;
    const load = () =>
      void bridge
        .readLog(name)
        .then((t) => {
          if (alive) {
            setText(t);
            setError("");
          }
        })
        .catch((e) => {
          if (alive) setError(String(e));
        });
    load();
    const timer = setInterval(load, 4000);
    return () => {
      alive = false;
      clearInterval(timer);
    };
  }, [name]);
  const visible = files
    .filter((f) => siteLogMatches(f, site))
    .sort(
      (a, b) => logLabel(a).localeCompare(logLabel(b)) || a.localeCompare(b),
    );
  return (
    <section className="panel">
      <div className="panel-header">
        <h2>{site ? `${site.hostname} logs` : "Logs"}</h2>
        <label>
          Log stream{" "}
          <select
            value={name}
            onChange={(e) => {
              setName(e.target.value);
              setText("");
              setError("");
            }}
          >
            <option value="">Choose a stream</option>
            {visible.map((f) => (
              <option key={f} value={f}>
                {logLabel(f)} - {f}
              </option>
            ))}
          </select>
        </label>
      </div>
      <ErrorNotice error={error} />
      <div className="panel-header">
        <p>Newest entries appear at the bottom. Showing the latest 128 KiB.</p>
        <button
          disabled={!text}
          onClick={() =>
            output.current?.scrollTo({
              top: output.current.scrollHeight,
              behavior: "smooth",
            })
          }
        >
          Jump to newest
        </button>
      </div>
      <pre className="log-output" ref={output}>
        {!visible.length
          ? "No logs yet. Start a site or create a project to see its output."
          : !name
            ? "Choose Web, PHP, Node, Vite, MySQL or a task stream above."
            : text || "This stream has no entries yet."}
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
