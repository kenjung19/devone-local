import { useState } from "react";
import type {
  Action,
  EnvironmentProfile,
  ProfileBinding,
  Snapshot,
} from "../contracts";

const emptyProfile = (): EnvironmentProfile => ({
  id: "",
  name: "",
  description: "",
  bindings: {
    web_server: { provider: "caddy", version: null },
    php: { provider: "php", version: "none" },
    mysql: { provider: "mysql", version: "none" },
    node: { provider: "node", version: "none" },
  },
  tools: {},
  built_in: false,
  created_at: 0,
  updated_at: 0,
});

function summary(profile: EnvironmentProfile) {
  return [
    ...Object.entries(profile.bindings).map(([kind, value]) =>
      kind === "web_server"
        ? `${value.provider} ${value.version ?? "default"}`
        : `${kind} ${value.version === "none" ? "None" : value.version ?? "default"}`,
    ),
      ...Object.entries(profile.tools).map(([tool, version]) =>
        `${tool} ${version === "none" ? "None" : version ?? "default"}`,
      ),
  ]
    .join(" / ");
}

export function Profiles({
  data,
  busy,
  act,
}: {
  data: Snapshot;
  busy: boolean;
  act: (action: Action) => Promise<boolean>;
}) {
  const [editing, setEditing] = useState<EnvironmentProfile | null>(null);
  const setBinding = (kind: string, binding: ProfileBinding | null) => {
    if (!editing) return;
    const bindings = { ...editing.bindings };
    if (binding) bindings[kind] = binding;
    else delete bindings[kind];
    setEditing({ ...editing, bindings });
  };
  const versions = (kind: string) => [
    ...new Set(
      [...data.installed.map((item) => item.manifest), ...data.available]
        .filter((item) => item.runtime === kind)
        .map((item) => item.version),
    ),
  ];
  const toolVersions = (tool: string) => [
    ...new Set(
      [...(data.tools ?? []), ...(data.available_tools ?? [])]
        .filter((item) => item.id === tool)
        .map((item) => item.version),
    ),
  ];
  return (
    <>
      <section className="panel">
        <div className="section-heading">
          <div>
            <h2>Environment Profiles</h2>
            <p>Presets copy exact bindings into one site. They never become a global active environment.</p>
          </div>
          <button className="primary" onClick={() => setEditing(emptyProfile())}>
            New Profile
          </button>
        </div>
        {(data.profiles ?? []).map((profile) => (
          <div className="runtime-row" key={profile.id}>
            <div>
              <strong>{profile.name}</strong>
              <p>{profile.description || summary(profile)}</p>
              <small>{summary(profile)}</small>
            </div>
            <div className="actions">
              {!profile.built_in && (
                <button onClick={() => setEditing(structuredClone(profile))}>
                  Edit
                </button>
              )}
              <button
                onClick={() =>
                  setEditing({
                    ...structuredClone(profile),
                    id: "",
                    name: `${profile.name} Copy`,
                    built_in: false,
                    created_at: 0,
                    updated_at: 0,
                  })
                }
              >
                Duplicate
              </button>
              {!profile.built_in && (
                <button
                  disabled={busy}
                  onClick={() => {
                    if (window.confirm(`Delete ${profile.name}? Sites already using it are unchanged.`))
                      void act({ type: "delete_profile", profile_id: profile.id });
                  }}
                >
                  Delete
                </button>
              )}
            </div>
          </div>
        ))}
      </section>
      {editing && (
        <section className="panel">
          <h2>{editing.id ? "Edit Profile" : "New Profile"}</h2>
          <div className="creation-fields">
            <label>
              Name
              <input value={editing.name} onChange={(event) => setEditing({ ...editing, name: event.target.value })} />
            </label>
            <label>
              Description
              <input value={editing.description} onChange={(event) => setEditing({ ...editing, description: event.target.value })} />
            </label>
            <label>
              Web Server
              <select
                value={editing.bindings.web_server?.provider ?? "caddy"}
                onChange={(event) => setBinding("web_server", { provider: event.target.value, version: null })}
              >
                <option value="caddy">Caddy</option>
                <option value="apache">Apache</option>
                <option value="nginx">Nginx</option>
              </select>
            </label>
            <label>
              Web Server version
              <select
                value={editing.bindings.web_server?.version ?? ""}
                onChange={(event) =>
                  setBinding("web_server", {
                    provider: editing.bindings.web_server?.provider ?? "caddy",
                    version: event.target.value || null,
                  })
                }
              >
                <option value="">Use provider default</option>
                {versions(editing.bindings.web_server?.provider ?? "caddy").map((version) => (
                  <option key={version}>{version}</option>
                ))}
              </select>
            </label>
            {(["php", "mysql", "node"] as const).map((kind) => (
              <label key={kind}>
                {kind.toUpperCase()}
                <select
                  value={editing.bindings[kind]?.version ?? "none"}
                  onChange={(event) =>
                    setBinding(
                      kind,
                      { provider: kind, version: event.target.value === "none" ? "none" : event.target.value || null },
                    )
                  }
                >
                  <option value="none">None</option>
                  <option value="">Use global default</option>
                  {versions(kind).map((version) => (
                    <option key={version}>{version}</option>
                  ))}
                </select>
              </label>
            ))}
            {(["composer", "pnpm"] as const).map((tool) => (
              <label key={tool}>
                {tool === "composer" ? "Composer" : "pnpm"}
                <select
                  value={
                    Object.prototype.hasOwnProperty.call(editing.tools, tool)
                      ? editing.tools[tool] ?? ""
                      : "unchanged"
                  }
                  onChange={(event) => {
                    const tools = { ...editing.tools };
                    if (event.target.value === "unchanged") delete tools[tool];
                    else tools[tool] = event.target.value || null;
                    setEditing({ ...editing, tools });
                  }}
                >
                  <option value="unchanged">Leave unchanged</option>
                  <option value="none">None</option>
                  <option value="">Use global default</option>
                  {toolVersions(tool).map((version) => (
                    <option key={version}>{version}</option>
                  ))}
                </select>
              </label>
            ))}
            <div className="actions">
              <button onClick={() => setEditing(null)}>Cancel</button>
              <button
                className="primary"
                disabled={busy || !editing.name.trim()}
                onClick={() => void act({ type: "save_profile", profile: editing }).then((ok) => ok && setEditing(null))}
              >
                Save Profile
              </button>
            </div>
          </div>
        </section>
      )}
    </>
  );
}
