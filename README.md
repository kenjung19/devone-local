# DEVONE Local

A Windows development environment for PHP, MySQL, Node.js and static projects. Run the runtime versions your projects need and open local sites through trusted HTTPS.

## Install

Use the current-user Windows x64 installer. Developer builds put it at `release/DEVONE-Local-0.1.0-Windows-x64-Setup.exe` with checksum metadata in `release/release-artifacts.json`. Binary installers are build output and are not committed to Git. Version 0.1.0 acceptance results and authorization policy are recorded in [Windows authorization readiness](docs/windows-authorization-readiness.md). Optional machine certification uses the [CMD acceptance workflows](scripts/acceptance/README.md).

## First run

Setup guides you through Home, Web Server, PHP, MySQL, Local Domains, HTTPS and readiness. Install only the versions you need. Node can wait until a Node project needs it. Enable automatic `.test` domains and HTTPS trust to open sites normally. Windows may request administrator permission for local domain setup.

Setup is a full page and can be reopened from Settings. If you skip an incomplete item, Sites explains what remains unavailable.

## Existing projects

1. Choose **Open www** and copy a project folder into it.
2. DEVONE discovers it automatically. No Add Site or template metadata is needed.
3. Open site details to choose PHP, Node or MySQL versions. Install missing dependencies when prompted.
4. Choose **Start**, then **Open Site**, **Editor** or **Terminal**.

Existing environment files are kept. Queue and scheduler workers require explicit enabling. Closing the window keeps DEVONE in the tray; **Quit** stops its owned processes.

## New projects

Choose **New Project**, select Blank PHP, Laravel, WordPress, Next.js, React + Vite or Static HTML, and enter a name. Relevant versions use defaults initially and can be changed explicitly. Optional dependency installation executes project install scripts.

Progress shows the current step. Failures preserve logs and explain whether a folder was retained. Creation does not start project servers or workers. WordPress finishes setup in its browser installer after you start the site.

## Runtime versions and tools

Runtimes shows Installed, Default and Available versions with explicit Install/Configure/Remove controls. Exact project selections are kept; there are no automatic upgrades. pnpm and Composer are CLI tools. Optional Mailpit captures local development email through Start/Stop/Open Mailbox. Choose your installed default editor in Settings.

## Databases and backups

Initialize/start MySQL, then refresh its database list. Managed databases support creation, connection information and manual backups. External and system databases have no destructive actions.

Restore requires trusted SQL and a typed target name. SQL may modify or overwrite existing objects; failures may leave partial changes. Back up first and use a disposable target when verifying a restore. Automatic scheduling and retention deletion are disabled.

## Where data is stored

Home defaults to `%LOCALAPPDATA%\Devone`; Settings shows the actual path. Set `DEVONE_HOME` or pass desktop `--home` with another absolute path before launch. A Home change requires restart.

| Folder | Contents |
| --- | --- |
| `www` | Projects |
| `runtimes`, `tools` | Managed binaries |
| `database` | Persistent MySQL and Mailpit data |
| `backups` | Manual SQL dumps and metadata |
| `config`, `certs`, `logs`, `cache` | Settings, HTTPS certificates, logs and caches |

Removing a project folder does not delete its database. Diagnostics is under Settings / Advanced. Review exported reports before sharing because they include local paths.

## Build for developers

Use the pinned toolchain; see [developer reference](docs/architecture-and-verification.md).

```powershell
. ./scripts/workspace-env.ps1
pnpm install --frozen-lockfile
pnpm desktop
# Build installer:
pnpm release:windows
# Verify release or an installed executable:
node scripts/smoke-release.mjs
node scripts/smoke-release.mjs "C:/path/to/devone-local.exe"
```

The smoke probe uses the real binary in a disposable Home in headless lifecycle mode. It checks SQLite, discovery, single-instance and shutdown; it does not establish visual GUI or interactive installer acceptance.

See [Windows hardening](docs/windows-hardening.md), [Phase 2](docs/phase2-development.md) and [Phase 3](docs/phase3-development.md) for architecture, tests and operational limits.
