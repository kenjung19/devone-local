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
      {(["pnpm", "composer"] as const).map((id) => (
        <section className="panel" key={id}>
          <h2>{id === "pnpm" ? "pnpm" : "Composer"} versions</h2>
          <p>
            Managed default: {data.tool_defaults?.[id] ?? "not configured"}. A
            project-declared pnpm version takes precedence.
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
                data.defaults[id === "pnpm" ? "node" : "php"] ?? null;
              return (
                <div className="stat" key={`${id}:${t.version}`}>
                  <strong>
                    {id} {t.version}
                  </strong>
                  <span>
                    {installed ? "Installed" : "Available"}
                    {data.tool_defaults?.[id] === t.version ? " · Default" : ""}
                  </span>
                  <div className="panel-buttons">
                    {installed ? (
                      <>
                        {(["default", "validate", "remove"] as const).map(
                          (operation) => (
                            <button
                              key={operation}
                              disabled={busy}
                              onClick={() =>
                                void act({
                                  type: "tool_action",
                                  id,
                                  version: t.version,
                                  operation,
                                  runtime_version: runtime,
                                })
                              }
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
                        disabled={busy || !runtime}
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
          <p>
            {id === "composer"
              ? "Composer validation and project commands use selected managed PHP."
              : "No global pnpm is required. Missing declared versions are never substituted."}
          </p>
        </section>
      ))}
    </>
  );
}
