# DEVONE Local

A Windows development environment for PHP, MySQL, Node.js and static projects. Run the runtime versions your projects need and open local sites through trusted HTTPS.

## Install

Use the current-user Windows x64 installer. Developer builds put it at `release/DEVONE-Local-0.1.0-Windows-x64-Setup.exe` with checksum metadata in `release/release-artifacts.json`. Binary installers are build output and are not committed to Git. Version 0.1.0 acceptance results and authorization policy are recorded in [Windows authorization readiness](docs/windows-authorization-readiness.md). Optional machine certification uses the [CMD acceptance workflows](scripts/acceptance/README.md).

The full installer and `release/DEVONE-Local-0.1.0-Windows-x64-Portable.zip` include one PHP stable, MySQL LTS, Caddy stable and Node.js LTS version, pinned in `scripts/windows-bundle.json`. Extract the portable `devone-local` folder into a writable location and run its executable. These runtimes are already registered; additional versions can be installed later. Projects, database data, credentials and CA private keys are not bundled.

## First run

Setup guides you through Home, Web Server, PHP, MySQL, Local Domains, HTTPS and readiness. Full bundles already include the default runtime versions. Initialize MySQL when needed and enable automatic `.test` domains and HTTPS trust to open sites normally. Windows may request administrator permission for local domain setup.

Setup is a full page and can be reopened from Settings. If you skip an incomplete item, Sites explains what remains unavailable.

## HTTPS certificate authority

Fresh Homes use a DEVONE-generated P-256 root at `certs/devone-ca/root.crt` and `root.key`. Its critical name constraints permit DNS names under `test` and exclude all IPv4/IPv6 addresses. Caddy loads this root, manages its intermediate and renews local site certificates. Windows trust is scoped to CurrentUser\Root and exact CA identity; Caddy's automatic trust installation remains disabled.

Existing Homes keep their legacy Caddy CA until you select **Upgrade HTTPS certificate authority** in Settings / HTTPS, Sites or Setup. Windows may display a trust confirmation. Cancelling retains legacy trust and restores the legacy served chain; select Upgrade again to resume. The old exact owned root is removed from trust only after replacement trust is confirmed, and old CA/certificate files remain in `backups/legacy-ca-*`. Recreate CA generates another constrained root and preserves the previous files in backups.

If an interrupted upgrade cannot restore its legacy backup, Settings → HTTPS offers **Abandon upgrade and create new CA**, with explicit confirmation. It archives the current CA/PKI files and creates a new constrained root. Only roots matching recorded ownership can be removed from trust; an old trust entry whose certificate file is unavailable remains untouched. Install trust for the new CA afterwards. CA recreation records a recovery journal before moving files; startup restores the previous files after an interrupted recreation. Verify trust in Settings after recovery. Quit waits for CA file moves and recovery to finish safely.

The CA directory and private key have protected Windows ACLs for the current user and SYSTEM. These ACLs do not block same-user npm/Composer processes; the certificate name constraints are the control for a stolen key. They do not sandbox malicious project code or prevent it from changing the user's own trust settings. See [Windows hardening](docs/windows-hardening.md) for verification and legacy-upgrade checks.

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

Backups include table drop/create statements, routines, events and triggers, so they can restore over the same managed database. Restore requires trusted SQL and a typed target name. It can replace existing tables/data; failures may leave partial changes. Routine/trigger restoration also depends on the engine's permissions and binary logging configuration; DEVONE does not grant global privileges or change those settings. Back up first and use a disposable target when verifying a restore. Automatic scheduling and retention deletion are disabled.

Database restore preserves SQL `DEFINER` clauses for routines, events and triggers. Restore these backups into the same managed database and user. Restoring into a different database/user can fail after some SQL has already applied because the original definer account or privileges differ; no automatic definer rewriting is performed. Back up the target first.

## Where data is stored

On Windows, Home defaults to the folder containing the executable. For example, `D:\devone-local\devone-local.exe` uses `D:\devone-local\www` for projects and stores runtimes, databases and configuration alongside it. Put the application in a writable folder. Settings shows the actual Home path. Set `DEVONE_HOME` or pass desktop `--home` with another absolute path before launch to override it. A Home change requires restart. Existing data in `%LOCALAPPDATA%\Devone` is preserved and is not moved automatically; use an override to continue using that Home.

| Folder | Contents |
| --- | --- |
| `www` | Projects |
| `runtimes`, `tools` | Managed binaries |
| `database` | Persistent MySQL and Mailpit data |
| `backups` | Manual SQL dumps and metadata |
| `config`, `certs`, `logs`, `cache` | Settings, HTTPS certificates, logs and caches |

Removing a project folder does not delete its database. Diagnostics is under Settings / Advanced. Review exported reports before sharing because they include local paths.

Settings → General lets you choose a separate **Projects folder (www)** and register individual projects from other folders. New projects and Open www use the chosen root; registered external projects are scanned alongside its top-level folders. Stop the environment and project tasks before changing locations. Changes do not move or delete files, runtimes or databases. Removing an external folder from the list only stops its discovery; its saved site record and database bindings are retained. Paths are machine-local, must be existing absolute directories, and cannot contain symlinks/junctions. The watcher monitors these roots non-recursively and polls project markers; unavailable folders appear in discovery diagnostics.

## Build for developers

Use the pinned toolchain; see [developer reference](docs/architecture-and-verification.md).

Full Windows packaging also requires Python 3.11 or newer through `py -3`. `pnpm release:windows` downloads and verifies the pinned vendor archives, stages a fresh Home in the system temporary directory, and creates both the bundled installer and portable ZIP. It never packages an existing user Home. The staging drive needs space for the extracted runtimes; users do not need Python to run DEVONE.

```cmd
call scripts\workspace-env.cmd
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
