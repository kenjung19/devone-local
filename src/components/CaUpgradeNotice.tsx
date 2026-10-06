import type { Action, Snapshot } from "../contracts";

export function CaUpgradeNotice({
  data,
  busy,
  act,
}: {
  data: Snapshot;
  busy: boolean;
  act: (action: Action) => Promise<boolean>;
}) {
  if (!data.setup.ca_upgrade_pending) return null;
  return (
    <div className="alert" role="status">
      Restrict HTTPS trust to .test domains; your existing CA stays trusted until
      the upgrade succeeds.{" "}
      <button
        disabled={busy || !data.setup.caddy}
        onClick={() => void act({ type: "upgrade_ca" })}
      >
        Upgrade HTTPS certificate authority
      </button>
    </div>
  );
}
