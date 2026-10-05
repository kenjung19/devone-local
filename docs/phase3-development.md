# Phase 3 developer workflows

Continues main `f4a7c26` without replacing Phase 1/2 architecture or upgrading locked application/toolchain dependencies. Creation is optional; copied projects remain automatically discovered from www. No automatic commit or push.

## Quick App and templates

The bundled JSON catalog defines Blank PHP, Static HTML, Laravel 13.10.1, WordPress 7.1.2, Next 16.3.8 and React + Vite 8.3.2 with TypeScript. Rust owns template strategies, requirements, task state and execution; React only presents relevant selections. Next/React files are maintained built-in starters with exact dependency versions, rather than globally installed generators. New project dependency resolution creates the project's own lockfile; its transitive dependencies are not a prebundled reproducible dependency tree.

Project names use strict lower-case hostname-compatible segments and reject reserved Windows names, traversal, existing destinations, case-insensitive conflicts and normalized hostname conflicts. Exact selected runtime/tool versions are checked before execution. Missing versions have explicit install actions; no fallback version is substituted. Only intentional non-default runtime choices become local overrides.

Creation is an owned one-shot background task with status, stage, persistent logs, creation time, destination and site ID. Staging is under Home/cache/project-staging/<UUID> with an ownership marker. Source files are moved to www after preparing/configuring the starter successfully. Optional pnpm installation then runs at the final disabled project path, because Windows pnpm junctions contain absolute targets and cannot safely be moved from staging. Dependency failure/cancellation preserves this new disabled project for recovery. A disabled site record is registered before the atomic folder rename so the watcher cannot start a half-finalized new project. Completion presents View Project, Start Site, Open Site, Editor and Terminal. Web/frontend/worker processes remain disabled until explicit Start. Static has no runtime selectors or database requirements; Start Site uses only Caddy.

Cancellation terminates owned Job Object subprocesses and removes only contained owned staging. Cleanup rejects links, including nested links, and preserves unsafe staging for recovery rather than following a junction. Final projects are preserved if cancellation or database configuration fails after the folder commit; task UI shows the retained path. Interrupted tasks are cancelled on reopen and never replayed. Pending database records/credentials remain available for diagnosis after a provisioning failure.

## Laravel and WordPress

Laravel uses the selected PHP executable and exact managed Composer PHAR. Composer runs create-project for the pinned skeleton with --no-scripts and no alternate-PHP retry. Its creation home/cache is isolated from global Composer configuration. Only this explicitly created Laravel project receives initial APP_URL, file-backed session/cache and synchronous queue settings. DEVONE runs artisan key:generate explicitly for the new project. Existing discovered .env files are untouched. Optional Node dependency installation and Mailpit configuration require explicit wizard selections. Queue/scheduler remain disabled. PHP extensions and dependency constraints are enforced by the selected Composer/PHP; incompatibilities appear in task logs.

WordPress downloads a SHA-256-pinned archive from WordPress.org, rejects unsafe archive entries, creates a database-specific managed user on selected MySQL and generates wp-config.php with local connection details and fresh salts. Passwords do not enter creation command arguments or logs. Start Site and finish the WordPress installer in the browser; no unattended admin account/site-content setup is claimed.

## Custom templates and editors

Local JSON templates live in Home/config/templates. They require custom- IDs, custom strategy, relative files, declared runtimes/tools, and structured managed node/php/pnpm/composer commands. Remote zip sources require HTTPS plus exact SHA-256. Templates are never executed by discovery. Create explicitly authorizes executing their code under the user's account; containment/Job ownership are not an OS sandbox. No marketplace or remote template service is required.

EditorProvider separates detection/opening from platform-neutral metadata. Windows detects known installed VS Code, Cursor, PhpStorm, WebStorm locations, plus database clients where available. Custom executable/JSON argument-array integration and the default editor are SQLite machine-local settings; absolute editor paths never enter .devone.json. {project} is the supported placeholder. Database clients open separately with no credentials in arguments. Site actions expose a default editor plus installed alternatives. Detection covers known installation layouts, not every possible Toolbox/custom installation; custom integration is the fallback.

Recent opened/created timestamps and favorites are presentation metadata in SQLite. Favorites sort first, then recent activity; discovery records remain visible regardless of recency. Tray adds New Project to open the full app page and site submenus for Open Site/Folder/Editor/Terminal. No destructive database actions appear in tray.

## Database administration and backups

The Database page keeps existing instance Start/Stop/Restart/Validate controls, data path and port, with logs and explicit actual-server metadata refresh. User databases and protected system databases are separate. Only DEVONE-managed databases have backup/restore/delete controls; unrelated databases are read-only. Cached metadata is explicitly labelled as the last queried result.

Create validates database/user names and refuses existing database/user adoption. It creates a generated protected password and database-specific grants, without global privileges. Delete requires typing the exact database name, rejects system/unmanaged databases and requires stopping a bound running site. Removing a project directory never deletes its database. Backups and encrypted credential artifacts are retained for explicit recovery.

Manual logical backups use the exact managed mysqldump, database-specific credentials, timestamp/UUID output under Home/backups/mysql/<database>, temporary output followed by rename, SHA-256, SQLite metadata and a JSON metadata sidecar. Passwords are not in command arguments/logs: managed clients receive a process-local MYSQL_PWD. Export uses --single-transaction/--no-tablespaces/--skip-lock-tables and no automatic DROP TABLE output. Consistency is best for transactional tables; changing nontransactional tables may be inconsistent.

Restore uses an explicit native .sql file chooser and typed target confirmation. It validates managed target/file/checksum for known backups, uses --binary-mode with the database-specific user, and never automatically drops the database. Trusted SQL can overwrite/merge/drop target objects according to its statements. Errors/timeouts can leave partial changes; restore is not transactionally atomic for DDL. Existing table structures may conflict with a no-DROP dump: restore into a new managed database or prepare the target explicitly. Credentials are omitted from subprocess error output. MySQL tools must exist in the selected distribution; unsupported external administrator authentication remains governed by the existing local-instance policy.

Manual backup works first. Retention preference is saved for future scheduling; automatic scheduling and deletion are disabled. Source-code backup, cloud backup and SQL-editor features are outside this phase. Optional Adminer/phpMyAdmin remain future managed tools.

## Mail, settings and diagnostics

Mailpit 1.31.3 is an optional pinned service tool. Install/validate/remove/default are separate from Start/Stop/Open Mailbox. SMTP and web UI use managed loopback ports with persistent local mailbox data, allowed-host restriction, version checks disabled and no relay configured. Existing PHP/Laravel sites receive suggested MAIL_* values only. Newly-created Laravel may opt into configuration while Mailpit is running. Mail port changes require explicit project configuration updates; existing .env files are not rewritten.

Settings group General, Startup, Local Domains, HTTPS, Editors, Templates, Backups and Runtime Catalog; advanced catalog sources remain collapsed. Diagnostics checks local Home, DNS policy/resolver/system lookup, CA, services, runtime files, installed tools and ports. Reports can be copied/exported locally as JSON; no automatic upload occurs. Report construction excludes project environment/log contents and redacts password/token/secret/credential/private-key fields recursively. Reports deliberately retain useful local paths, with a warning to review before sharing.

## Verification

Frontend frozen install, lint, typecheck, 18 tests and production build passed. Rust fmt, clippy for all targets with warnings denied, and the default offline suite passed: 50 tests across library, desktop preferences, Phase 2 and Phase 3. The default suite leaves eight environment-dependent native tests ignored; it does not require public internet.

All nine owned process lifecycle tests passed with RUST_TEST_THREADS=1, including background project creation cancellation. A parallel rerun under concurrent build load exposed a timing-sensitive immediate-worker-failure assertion; the complete serialized rerun passed without changing the existing health policy.

Native checks passed for Blank PHP and Laravel creation, Next and React/Vite creation followed by actual managed HTTPS startup, Mailpit installation/version validation/SMTP EHLO/web UI/owned stop, and MySQL 8.4.11 backup/restore/checksum/delete. WordPress creation against that MySQL fixture passed in the earlier combined run; a later repeat reached the optional WordPress download and failed on a network request after all database assertions passed. The final database-only rerun passed. The generated React/Vite starter also passed a production build using its selected managed Node/pnpm.

Phase 2 Node/static HTTPS and reopen regression passed. Actual Vite, Next and Laravel/Vite HTTPS + HMR protocol regression passed in 77.19 seconds. The latter used a fresh Home with newly issued certificates and checksum-verified copies of previously installed exact managed PHP/pnpm/Composer tools. This avoided an expired certificate in an older test Home and network bootstrap delays; TLS verification and actual tool version validation remained enabled.

The final pnpm release:windows build passed on 2026-10-05 at 21:02 Asia/Bangkok. The current-user x64 NSIS installer is src-tauri/target/x86_64-pc-windows-msvc/release/bundle/nsis/DEVONE Local_0.1.0_x64-setup.exe (8,905,601 bytes). SHA-256: 92b8e061081f3c9e7c8fb24f1a42dada1fb084bc7a95bc27abccc6e80f49cc63. release-artifacts.json records the matching checksum and payload audit. Payload statements contain only the product executable, devone-core and WebView2 support; fixtures, temporary projects, Home, backups, node_modules and debug binaries are excluded. Template/tool catalogs are compiled product resources. This verifies packaging, not an interactive installer execution. Manual GUI acceptance, installed editor launches, the native SQL chooser interaction, browser WordPress setup and visual browser Fast Refresh acceptance are deferred and are not claimed by these automated checks.

## Deferred desktop acceptance

Use a disposable Home and database for these manual checks:

1. Install the NSIS build, verify tray/single-instance behavior and open New Project from the tray.
2. Walk through the full-page creation flow for each template; inspect progress, cancel, retained-path recovery and explicit Start/Open actions.
3. Open a created project with detected/default/custom editors and terminal; reopen the app to verify recent/favorite/default persistence.
4. Initialize/start MySQL, refresh actual metadata, create a managed database, take a backup, choose its SQL through the native file dialog and restore into a new disposable managed target. Verify exact typed deletion guards and protected/unrelated database presentation.
5. Start Mailpit, send mail from an opted-in new Laravel project and inspect the mailbox. Check that existing project environment files remain unchanged.
6. Review grouped Settings and diagnostic copy/export, then finish a created WordPress site in its browser installer and check browser-visible Vite/Next refresh behavior.

These interactions remain deferred; the native protocol and lifecycle tests above do not establish visual GUI acceptance.

## Operational limits

Creation dependencies and official archives may require network access. Custom template commands run with the user's privileges and must be trusted. External editor detection covers known installation layouts. SQL restore may partially apply changes, and no-DROP dumps may conflict with existing tables. Backup/restore subprocesses currently have a 120-second limit; restore accepts files up to 512 MiB, and backup checksum verification currently reads the complete dump into memory. These limits should be considered for large databases. Retention is a saved foundation only: no automatic backup or deletion is enabled.

## Verdict

PHASE 3 COMPLETE

Required implementation and automated verification are complete, with no remaining Phase 3 implementation blocker. Manual desktop acceptance remains deferred as explicitly allowed by the task. Locked application/toolchain dependencies are unchanged. Commit and push require an explicit user instruction.

## Primary references

- [Laravel skeleton metadata](https://packagist.org/packages/laravel/laravel)
- [Composer create-project](https://getcomposer.org/doc/03-cli.md#create-project)
- [WordPress official releases](https://wordpress.org/download/releases/)
- [Mailpit official release](https://github.com/axllent/mailpit/releases/tag/v1.31.3)
- [Mailpit runtime options](https://mailpit.axllent.org/docs/configuration/runtime-options/)
- [MySQL mysqldump](https://dev.mysql.com/doc/refman/8.4/en/mysqldump.html)
- [MySQL client options](https://dev.mysql.com/doc/refman/8.4/en/mysql-command-options.html)
