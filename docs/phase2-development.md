# Phase 2 modern development workflow

DEVONE Local remains **React + Vite + Tauri/Rust**. Next.js and Laravel Vite are supported user-project adapters; the native acceptance projects are test fixtures, not application dependencies. Phase 1 and Phase 2 architecture and locked toolchain versions are preserved. Project folders are discovered from `www`; Add Site is not required.

## Frameworks and routing

Discovery order remains Laravel, Next, Vite, generic Node, Static, PHP fallback. Discovery reads metadata only. It never runs package/composer scripts, artisan, installs dependencies, enables processes, or executes project configuration. Invalid metadata/config produces a visible error and blocks project execution.

Next starts its known `dev`/`start` script through selected site Node and exact managed pnpm. The adapter passes Next's supported `--hostname 127.0.0.1 --port <managed port>` arguments. It never rewrites the contents of a package script. Caddy terminates local HTTPS and proxies HTTP and WebSocket to the managed loopback listener.

Recent Next versions enforce development-origin checks on HMR. For Next development endpoints (`/_next/*`, `/__nextjs*`) Caddy translates **only** `Origin: https://<this site>` to the loopback upstream origin. Application requests, Server Actions and foreign origins retain their headers. This avoids modifying user Next configuration or widening its origin allowlist. Tests exercise the actual Next 16.3.8 `/_next/hmr` endpoint, including a rejected foreign origin. Custom asset prefixes/configurations may require explicit project integration.

Vite starts a known script with loopback host, managed port and strictPort. DEVONE generates a development-only config in `Home/config/vite-<site id>.mjs`. On explicit Start, this loads the project's Vite config with Vite's own loader and applies the managed server settings: HTTP loopback upstream, HTTPS site origin, the site hostname as the additional allowed host, exact CORS origin and WSS HMR hostname/client port 443. Product runtime flags apply only to detected web/frontend processes, not arbitrary custom pnpm processes. Source configuration is not edited.

Static sites use Caddy file serving with no PHP, Node or MySQL default requirement. Generic Node projects offer known dev/start script choices as local overrides. Generic applications must honor managed PORT/HOST; they can use a structured custom `web` definition when the package scripts are unsuitable. Discovery never parses arbitrary shell script internals.

## Laravel + Vite ownership

Laravel PHP uses the public document root and its selected managed PHP FastCGI runtime. A Vite suggestion is created only when the project actually declares Vite. Queue Worker and Scheduler remain disabled suggestions until explicit Start.

Laravel Vite uses a managed `/__devone_vite/` base. The real Laravel Vite plugin writes `public/hot` as `https://<site>/__devone_vite`. Caddy sends this prefix to the healthy managed Vite listener and sends other requests to PHP. Browser assets and HMR use HTTPS/WSS; the internal Vite listener remains HTTP on loopback.

Before starting, DEVONE refuses an existing unowned hot file. It records the expected hot URL in a Home-side ownership marker and validates that the plugin produced it. Stop, failed startup, shutdown and reopen remove hot content only if it matches the recorded managed value. Changed/user-owned content is preserved. Links and project escapes are rejected. Port changes regenerate the runtime config and Caddy upstream; the browser-facing hot URL remains stable. Restarting Vite preserves the PHP PID and does not restart MySQL. Customized hotFile/public directory behavior is not guessed: incompatible output produces an actionable error.

## Process lifecycle and UX

Structured definitions contain id, name, runtime, managed executable, args array, relative cwd, environment, port, optional health strategy and autostart. Supported executables remain node, pnpm, php and composer. Processes use `site:<site id>:<process id>` keys and owned Windows Job Objects. Definitions and enabled state are persisted in SQLite; ports and service state retain the existing managers.

Health strategies are ProcessAlive, TcpListener and Http. ProcessAlive uses a short startup stability window to catch immediate worker failures. TCP/HTTP require an owned managed listener. Web defaults to HTTP readiness with bounded requests/startup timeout; workers default to process alive. Failed web health withdraws Caddy routing while retaining the site/hostname and logs. Recovery is bounded, and Restart remains explicit. Port reassignment is reflected in Caddy without routing to a foreign listener.

The site page groups Web, Frontend, Workers and Custom, with human labels, Start/Stop/Restart, autostart toggles, Disable process, Edit, Reset/Remove and Logs. Enabled and autostart are independent:

- First discovery: defined, disabled, stopped.
- Successful explicit Start: enabled, running.
- Stop process: stopped for this controller session; definition/authorization retained.
- Disable process: enabled=false; no automatic launch.
- Reopen with autostart=false: enabled but stopped.
- Reopen with autostart=true: reconciled only when environment autostart is enabled.
- Editing a command revokes enabled authorization; changing only autostart does not revoke it or restart a running command.

Start Site starts required route services and previously enabled project processes. Optional process failures do not invalidate an unrelated PHP route. Stop Site stops its owned project processes and routing, preserves definitions/configuration and leaves shared PHP/MySQL running when other sites need them. Site stop preference survives reopen. Global Stop preserves definitions and authorization.

## Managed tools

Tools catalog supports simultaneous pnpm 10.30.1, 11.0.0 and 12.8.1, plus Composer 2.2.26 and 2.10.3. Artifacts are exact SHA-256 pinned. pnpm 12 includes its separately verified native Windows payload; pnpm 10/11 use their version-specific Node CLI entries. Bootstrapping uses selected official Node's bundled npm only to unpack verified local archives, offline and with scripts disabled. No global pnpm is required, and DEVONE development remains on its existing pnpm 12.8.1 lock.

The Tools page displays installed/available versions and offers Install, Validate, Remove and Set default. Exact project pnpm intent wins over the managed default. Missing declared versions are never substituted; versions absent from the pinned catalog are identified honestly. Removing the default clears it and requires explicit replacement rather than silently choosing another installed tool. Old foundation installs receive a one-time default migration. A newly installed first tool becomes its managed default; changing defaults applies on the next launch. Enabled projects, running processes using an older version, and running dependency tasks guard removal.

pnpm version management is disabled with version-appropriate CLI policy: v10 `--config.manage-package-manager-versions=false`, v11/12 `--pm-on-fail=ignore`. Environment policy also prevents terminal shims from implicit version downloads. Parent workspace discovery is disabled when a project does not contain its own pnpm-workspace.yaml, preventing a nested project install from operating on an unrelated enclosing workspace.

Composer is a managed PHAR, invoked using the site's selected PHP, never global PHP. Install and Validate check the exact Composer version; project commands validate compatibility first. Incompatibility identifies the Composer and PHP versions and does not silently change either. Current Composer selection is the explicit managed default; per-site Composer selection can be added later without changing the model.

## Metadata, dependency tasks and safety

Package manager intent considers packageManager first, then pnpm/npm/yarn/bun lockfiles. Conflicts are visible and block automated installation. No lockfiles are deleted or converted. npm/yarn/bun remain detectable with an explicit terminal-only automated-install message.

Node and Composer dependency state is Installed, Missing, Possibly stale or Unknown. Staleness is a conservative timestamp heuristic comparing manifests/lockfiles with node_modules or vendor/autoload.php, not a guarantee that an install is necessary. File changes never trigger an install.

Install Dependencies is a separate one-shot owned task with running/completed/failed/cancelled state, persistent logs and Home-side task state. It does not use service recovery. Cancel terminates only owned descendants; shutdown cancels owned tasks. Reopen marks an interrupted task cancelled and never replays it. Completion logs remain accessible. Installs run project code only after explicit user action.

The editor validates runtime/tool availability, cwd existence and containment, unique/reserved IDs, structured arguments and environment keys before saving. PATH, PORT, HOST, PHPRC, DEVONE_HOME, DEVONE_SITE and additional managed settings cannot be overridden. Secret-looking keys are rejected in this iteration for both local and portable definitions; this is a conservative policy, not a secrets manager. Local environment values are machine-local until the user explicitly exports permitted portable configuration.

`.devone.json` schema v1 has schema_version, runtimes and processes only. Unknown fields, malformed exact runtime refs, unsupported executables, duplicate process IDs, unsafe cwd, invalid arguments and forbidden environment keys are errors. It has no enabled state, PIDs, allocated ports, credentials or absolute executable paths. Precedence is local SQLite override, portable intent, detected recommendation, global default where applicable. Existing project database binding precedence is preserved. UI distinguishes Local override, .devone.json, Global default and Project database binding, and separates Save local override from Save portable config.

Execution is structured; DEVONE does not construct arbitrary shell command strings. Explicitly started package scripts still execute user project code under the user's account; Job ownership and path validation are not an OS sandbox.

## Verification (2026-10-04–2026-10-05)

Frontend frozen install, lint, typecheck, tests and build passed; 15 frontend tests. Rust fmt and clippy with warnings denied passed. Rust tests passed: 29 unit tests, 1 desktop preference test, 13 offline Phase 2 tests, 8 owned process/task lifecycle tests and 2 explicit native integrations (53 total). All final code was verified after the startup-stability and active-tool guards. Native Node baseline passes runtime isolation, HTTPS/static routing, unexpected web-exit route removal, occupied-port reassignment, restart, failed first Start, reopen and site stop persistence. Native real-framework protocols passed Vite 8.3.2, Next 16.3.8, Laravel Vite plugin 3.2.0 and PHP 8.5.11 with managed pnpm 10/11 and both Composer versions. Vite and Next received WebSocket HMR notifications after source edits; Laravel hot cleanup and PHP PID preservation passed. Shared PHP Stop Site regression is included.

Protocol-level tests do not claim browser-rendered Fast Refresh/client navigation or full Laravel application/vendor/queue acceptance. No GUI/browser manual observations were made. The complete multi-version native PHP/MySQL workflow is conditional on its original fixtures and was not rerun; available PHP plus Phase 1 offline regressions were exercised. Deferred Phase 1 GUI acceptance remains deferred.

Windows release build and generated NSIS payload audit passed on 2026-10-05. The NSIS current-user installer is `src-tauri/target/x86_64-pc-windows-msvc/release/bundle/nsis/DEVONE Local_0.1.0_x64-setup.exe` (8,685,097 bytes), SHA-256 `aae32e44ff3680efad7cdd6a9b09077a70ff8625761c7cba6bdcb2db09be62dd`. `release-artifacts.json` records the allowed File payload statements and hash. Manual installation was not performed. The release script rejects fixture/debug binaries, node_modules, project dependencies and DEVONE_HOME payloads. Only product executables and standard WebView installer resources are allowed; catalogs and frontend are product assets embedded by the existing architecture.

Verdict: **PHASE 2 COMPLETE** for the requested implementation and automated/protocol-level acceptance. There are no remaining Phase 2 blockers under the agreed acceptance scope. Deferred manual GUI/browser observations and unavailable full multi-version PHP/MySQL fixtures remain explicitly unverified as described above. Locked application/toolchain dependencies are unchanged. No commit or push was performed.
