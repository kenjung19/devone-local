import type { Snapshot, Action } from "../contracts";
export function ToolManager({
  data,
  busy,
  act,
}: {
  data: Snapshot;
  busy: boolean;
  act: (a: Action) => Promise<void>;
}) {
  return (
    <>
      {(["pnpm", "composer", "mailpit"] as const).map((id) => (
        <section className="panel" key={id}>
          <h2>
            {id === "pnpm"
              ? "pnpm"
              : id === "mailpit"
                ? "Mailpit (service tool)"
                : "Composer"}{" "}
            versions
          </h2>
          <p>
            {id === "mailpit"
              ? `Local mail service - ${data.developer?.mail.running ? "Running" : "Stopped"}`
              : `CLI tool - Default: ${data.tool_defaults?.[id] ?? "not configured"}`}
          </p>
          {[
            ...new Map(
              [...(data.available_tools ?? []), ...(data.tools ?? [])].map(
                (t) => [`${t.id}:${t.version}`, t],
              ),
            ).values(),
          ]
            .filter((t) => t.id === id)
            .map((t) => {
              const installed = (data.tools ?? []).some(
                (v) => v.id === id && v.version === t.version,
              );
              const runtime =
                id === "mailpit"
                  ? ""
                  : (data.defaults[id === "pnpm" ? "node" : "php"] ?? null);
              return (
                <div className="stat" key={`${id}:${t.version}`}>
                  <strong>
                    {id} {t.version}
                  </strong>
                  <span>
                    {installed ? "Installed" : "Available"}
                    {id !== "mailpit" && data.tool_defaults?.[id] === t.version
                      ? " · Default"
                      : ""}
                  </span>
                  <div className="panel-buttons">
                    {installed ? (
                      <>
                        {(["default", "validate", "remove"] as const).map(
                          (operation) =>
                            !(id === "mailpit" && operation === "default") && (
                              <button
                                key={operation}
                                disabled={busy}
                                onClick={() => {
                                  if (
                                    operation === "remove" &&
                                    !window.confirm(
                                      `Remove ${id} ${t.version}? Project files and mail data are kept.`,
                                    )
                                  )
                                    return;
                                  void act({
                                    type: "tool_action",
                                    id,
                                    version: t.version,
                                    operation,
                                    runtime_version: runtime,
                                  });
                                }}
                              >
                                {operation === "default"
                                  ? "Set default"
                                  : operation === "validate"
                                    ? "Validate"
                                    : "Remove"}
                              </button>
                            ),
                        )}
                      </>
                    ) : (
                      <button
                        disabled={busy || (id !== "mailpit" && !runtime)}
                        onClick={() =>
                          void act({
                            type: "install_tool",
                            id,
                            version: t.version,
                            node: runtime,
                          })
                        }
                      >
                        Install {id} {t.version}
                      </button>
                    )}
                  </div>
                </div>
              );
            })}
          {id === "mailpit" && (
            <div className="panel-buttons">
              <p>
                Optional loopback-only mail service. SMTP{" "}
                {data.developer?.mail.smtp_port ?? "not allocated"} · Mailbox{" "}
                {data.developer?.mail.web_port ?? "not allocated"}
              </p>
              {(["start", "stop", "open"] as const).map((operation) => (
                <button
                  key={operation}
                  disabled={
                    busy ||
                    !data.developer?.mail.installed ||
                    (operation === "start" && data.developer?.mail.running) ||
                    (operation === "stop" && !data.developer?.mail.running) ||
                    (operation === "open" && !data.developer?.mail.running)
                  }
                  onClick={() => void act({ type: "mail", operation })}
                >
                  {operation === "open"
                    ? "Open Mailbox"
                    : operation === "start"
                      ? "Start"
                      : "Stop"}
                </button>
              ))}
            </div>
          )}
          <p>
            {id === "mailpit"
              ? "Capture development email locally. Existing project mail settings are kept."
              : id === "composer"
                ? "Composer validation and project commands use selected managed PHP."
                : "No global pnpm is required. Missing declared versions are never substituted."}
          </p>
        </section>
      ))}
    </>
  );
}
