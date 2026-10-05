# Windows product hardening

Continues main f18b88d without Phase 4 features, version bumps or locked dependency/toolchain upgrades. Application version remains 0.1.0. No automatic commit/push.

## Journey review and changes

| Area | Issue found | Result |
| --- | --- | --- |
| Navigation | Diagnostics competed with daily tasks | Seven main areas: Sites, New Project, Runtimes, Databases, Tools, Logs, Settings; Diagnostics under Settings / Advanced |
| Sites | Rows offered only navigation, hid hybrid runtimes and missing folders | Cards show applicable runtimes, meaningful status, Start/Stop, Open Site, detected editor, Terminal, Details and actionable problems |
| Empty workspace | Copy workflow was not paired with project creation | Open www / New Project with one sentence explaining discovery; no Add Site |
| Site health | Shared web infrastructure and disabled web state could admit misleading health; enabled Laravel/Vite was not gated | Exact route plus owned required web services; enabled Vite health gates the hybrid site while optional workers remain separate |
| Runtimes | Internal paths/FastCGI descriptions dominated | Installed/Default/Available, catalog channel labels, explicit install/remove; advanced metadata/import expandable |
| Tools | Mailpit displayed CLI default concepts and generic pnpm instructions | CLI versus local mail service; Mailpit Running/Stopped, Start/Stop/Open Mailbox and no default control |
| Creation | Plain selector, raw task stages and stage overwritten on failure | Template cards, relevant fields, defaults/minimum-compatible initial versions, readable progress, preserved failed stage/logs, retained-folder explanation and safe retry setup |
| Database | Dangerous controls and backup metadata dominated | Instance health and initialization controls, managed/external/system labels, Backup Now and last successful backup; connection/restore/delete and backup history expandable |
| Setup | Caddy/DNS implementation names in step navigation | Web Server / Local Domains; same full-page seven-step architecture and explicit skip readiness |
| Editor | Sole detected editor required setting a default before opening | Sole editor opens by name; multi-editor default + alternatives; custom paths in Advanced |
| Logs | Default combined stream obscured origin | Labelled separate streams, explicit choice/empty state and Jump to newest; latest 128 KiB with newest at bottom |
| Tray | No per-site lifecycle actions and daily global controls buried | Per-site Start/Stop, global Start All/Stop All, Open www, New Project, no destructive database/runtime actions |
| Errors | Raw internal messages in primary action area | Action-oriented summary with expandable full technical details; operation status while blocking actions run |
| Confirmations | DNS/trust/tool removal lacked clear intent | Confirm relevant removal and creation cancellation after commit; Start/Stop stays immediate |
| Desktop basics | Checkbox sizing, wrap, focus and dialog semantics | Laptop breakpoints, wrapped controls, visible keyboard focus, labels, dialog role/title/Escape and alert/status semantics |
| README | Engineering status dominated installation/daily workflow | Product-first Install/First Run/Existing/New/Versions/Data/Build; old engineering detail moved to architecture-and-verification.md |

Project discovery remains independent of creation metadata and never executes generators. Runtime selection remains exact and no existing .env is rewritten. Advanced process configuration remains available in site details. New project web/frontend/workers stay disabled until explicit Start.

## Release output and guard

`pnpm release:windows` retains the original NSIS output and copies the checksummed installer to ignored `release/DEVONE-Local-0.1.0-Windows-x64-Setup.exe`, with matching release-artifacts.json. The payload allowlist covers the main executable, devone-core and WebView2 support only, rejecting tests, fixtures, staging, Home, backups, node_modules and debug binaries. Copy output validates names/version, rejects linked output paths and verifies bytes/SHA-256. Regression tests exercise payload rejection, metadata consistency, checksum failure and filename traversal. Binary installers are not Git files.

## Smoke harnesses

Run `node scripts/smoke-release.mjs [absolute-installed-executable-path]`. It launches the real release/installed executable with `--smoke-test --home <new disposable Home>`. This explicit headless probe uses the real DesktopInstance lock/activation, Application/SQLite, watcher and shutdown code with system DNS/autostart disabled. It verifies Home/SQLite, copies a project while running and observes watcher discovery, verifies the second instance activates/exits without replacing the primary, then requests token-confirmed shutdown. Smoke mode rejects existing database/project Homes and has a bounded lifetime. Evidence remains in its temporary Home.

This probe does not initialize WebView or verify interactive installer/UI behavior. Manual desktop acceptance remains deferred; do not infer it from headless smoke.

`cargo test --manifest-path src-tauri/Cargo.toml --test product_smoke -- --ignored --nocapture` with DEVONE_PRODUCT_SMOKE_ROOT selects the prepared disposable Phase 2 fixture Home with exact managed PHP/Node/Caddy/pnpm. It copies PHP and Vite projects into www without .devone.json, discovers disabled processes, installs Vite dependencies from the offline store at their final path, starts sites, verifies actual HTTPS responses against the Home CA, stops and shuts down. Missing dependency/store fixtures fail explicitly.

The existing ignored Phase 3 MySQL native test covers initialization, managed create, insert/read, backup, restore/checksum guard, protected/unrelated databases, deletion guard and owned stop. It is run separately without WordPress network setup for this hardening pass.

## Verification

- Frozen frontend install, lint/typecheck, 25 tests and production build: passed.
- Rust fmt, all-target clippy with warnings denied, 52 offline tests: passed.
- Owned process lifecycle: all 9 passed with RUST_TEST_THREADS=1.
- Native copied PHP/Vite smoke: passed in 10.31 seconds, including verified HTTPS and owned stop.
- Native MySQL initialization/create/read/backup/restore/delete protection/stop smoke: passed in 16.03 seconds.
- Existing Next/Vite/Laravel-Vite HTTPS and HMR protocol regression after the site health change: passed in 31.27 seconds.
- Final pnpm release:windows: passed, including the payload audit and checksum-verified root release copy. Installer: release/DEVONE-Local-0.1.0-Windows-x64-Setup.exe, 8,913,016 bytes; SHA-256 0e6bfcf45dfcac1ff1b6d568c5b75edf26f62b28d78b7a8f36d55e3fc75011d5.
- Real release executable headless lifecycle smoke: passed for Home/SQLite, live watcher discovery, second-instance activation with unchanged controller PID, and clean shutdown/activation endpoint cleanup. Evidence retained at .toolchain/tmp/devone-release-smoke-W180ze.
- Interactive installer execution, rendered desktop layouts and GUI/manual acceptance: deferred and not claimed.
- Dependency/lockfile changes: none.

## Desktop acceptance still required

Install/uninstall the current-user package and manually check first run, skipped setup, tray/window/single-instance, editor/terminal launches, file chooser, browser trust and refresh, keyboard dialog interaction, and layout at 1280x720, 1366x768 and 1920x1080. Automated markup tests and responsive CSS do not establish rendered visual acceptance.

Operational limits from Phase 3 remain: trusted SQL may leave partial changes; large database backup/restore limits; custom template commands run as the user; dependency downloads depend on network; editor detection covers known layouts; automatic backups are not enabled. No major feature category was added.

## Release acceptance verdict

READY FOR FINAL RELEASE ACCEPTANCE

No known implementation or automated-verification blocker remains for this hardening scope. Final everyday-use acceptance still requires the manual installed-desktop checks listed above. One more broad hardening prompt is not needed before acceptance; use concrete findings from that acceptance run for any further fixes. The smoke harness can also be pointed at an installed executable, but this run verified the built release executable only.
