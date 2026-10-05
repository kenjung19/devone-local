# Windows acceptance from CMD

These workflows use CMD. Run commands from the repository root. Keep the
application version and pinned development toolchains unchanged.

## Explicit administrator-only DNS acceptance

Open **Command Prompt → Run as administrator**, then run:

```cmd
D:\Project\devone-local\scripts\acceptance\windows-dns.cmd
```

The script checks elevation before any policy mutation. It opts into the ignored
native DNS test and uses the same fixed-scope implementation as the production
elevated helper. It refuses an existing DEVONE policy or a foreign `.test`
policy/listener. Its guard removes only its own policy, shuts down its resolver,
and verifies cleanup. It never kills a foreign listener or restores arbitrary
Windows policy state.

Evidence: `.devone-test/release-gates-20261005/elevated-dns-result.log`. A normal
CMD invocation fails before changing policy. Missing elevation is not a passing
integration result.

## Explicit current-user CA acceptance

From CMD run `scripts\acceptance\windows-ca.cmd`. This opts into the two native
system-CA tests. Windows may ask to trust each disposable root; approve only the
certificate belonging to the printed disposable Home. The printed SHA-256 is
the **CA file hash**, not a Windows SHA-1 certificate thumbprint. The guard
captures an immutable exact certificate and verifies the Root baseline after
each test, including failure/panic. A timeout is a failed acceptance result.
Output stays visible and is also saved to
`.devone-test/release-gates-20261005/current-user-ca-result.log`.
Fixtures come from the bundled catalog if the optional local fixture variables
are absent. Do not run these tests concurrently with another DEVONE instance
using ports 80/443 or another machine-integration acceptance test.

## Installed desktop command acceptance

Build a disposable installer with the explicit test-only feature:

```cmd
call scripts\workspace-env.cmd
pnpm exec tauri build --target x86_64-pc-windows-msvc --bundles nsis --features release-acceptance
```

Install it into a fresh disposable directory after checking that no unrelated
DEVONE Local uninstall registration would be replaced. Run:

```cmd
node scripts\acceptance\installed-ipc.mjs D:\path\to\disposable\Application\devone-local.exe
```

This creates its own Home and starts the actual Tauri app with Windows-only PATH.
The token-scoped, size-limited file client invokes the same shared application
action implementation as Tauri IPC. Close requests use the actual window event;
Quit uses the actual tray dispatcher. Tests cover discovery while hidden,
activation/show/focus requests, runtime/tool/database/project actions, setup
readiness and all four cold combinations of Windows startup/environment startup.
Existing unrelated startup registration is preserved and reported as untested.

The client refuses CA, DNS-policy, hosts and credential-reveal actions. Those
require separate native acceptance with exact machine-resource cleanup guards.
Results and failure details are saved beneath the disposable Home. This does not
claim visual menu clicks, foreground observation or viewport layout approval.

Uninstall only the verified disposable installation before building/installing
the normal release. **Do not distribute the instrumented installer.** The normal
`pnpm release:windows` command excludes the `release-acceptance` feature and
produces `release/DEVONE-Local-0.1.0-Windows-x64-Setup.exe`.

## Local Schannel test client

The native system-CA test uses `curl --ssl-no-revoke` only for its local offline
CA, which has no online CRL/OCSP endpoint. The option affects Schannel revocation
checking; root trust, chain/signature and hostname validation remain enabled.
Removing the exact root must make the same request fail. Product TLS does not
disable verification. See the [curl option documentation](https://curl.se/docs/manpage.html#--ssl-no-revoke)
and [Caddy local HTTPS documentation](https://caddyserver.com/docs/automatic-https#local-https).
