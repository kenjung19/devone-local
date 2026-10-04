# Phase 2: Node runtimes and project processes

## Architecture and state

Node reuses RuntimeManager installations, exact RuntimeRef bindings, catalog checksum verification, safe ZIP extraction, Home-relative paths and executable validation. It is not a globally installed developer tool. The Windows catalog contains Node 22.23.3, 24.21.0 and 26.10.0 for x64/arm64. PHP/MySQL/Caddy entries remain intact. Catalog URLs and checksums come from [Node distributions](https://nodejs.org/dist/), including each release's SHASUMS256.txt. Import validates `node --version`; remove protects default, explicit SQLite and portable bindings.

Adapter ordering is Laravel, Next, Vite, generic Node, Static, PHP fallback. Discovery reads metadata and never runs package scripts, installs dependencies, enables workers or executes portable definitions. Invalid metadata appears as an actionable error rather than silently becoming PHP.

Runtime precedence is explicit SQLite site override, portable exact `.devone.json` runtime intent, then global default. Missing exact Node versions remain selected. Runtime absence never silently substitutes another installed version. PHP and database bindings retain Phase 1 behavior. Node-only and Static sites do not inherit PHP/MySQL defaults.

Portable intent has schema version 1 and contains only exact runtime versions and structured process definitions. It has no enabled flag, PID, assigned port or machine executable paths. SQLite is canonical for local UI overrides, local custom definition edits and user authorization. Local definitions override portable/detected suggestions with the same process ID. Saving `.devone.json` explicitly exports the effective definitions and explicit bindings; it never exports defaults, enabled state, credentials or secrets. Process cwd and positional file arguments must be relative. Portable and local definitions reject environment secrets and managed PATH overrides. The optional file is never created by discovery.

A process uses `site:<site_id>:<process_id>`. Definition, enabled state and site stop preference are persisted in SQLite; existing port_allocations and process_state tables own assigned ports and status. Snapshot combines definition, runtime, executable symbol, args, cwd, environment, autostart, alive/owned-port health strategy and status. Commands resolve managed executable paths at execution time. Windows Job Objects and the original supervisor retain owned-child cleanup, stderr/stdout logs and bounded recovery. Foreign processes are never adopted or killed.

## Tools and execution

Managed pnpm 12.8.1 lives in `Home/tools/pnpm/12.8.1`. This release has a Node wrapper and a native pnpm binary. Both npm registry archives are SHA-256 pinned in tools-catalog.json. Installation uses the selected official Node distribution's bundled npm CLI only to unpack these already verified local tool archives, with scripts disabled and offline mode. DEVONE development continues to use pnpm only. No global npm/pnpm is needed. Tool install verifies the wrapper's exact reported version before registering it; COREPACK_ENABLE_NETWORK=0 prevents an implicit wrapper download. Project execution is selected site Node + managed pnpm wrapper, and its native pnpm child remains inside the owned Job.

Declared packageManager and lockfiles are inspected independently. Conflicting managers block managed installation. Existing npm/yarn projects are displayed honestly and are not converted. Their project execution/dependency installation remains a terminal workflow; pnpm is used for undeclared or pnpm projects only. A declared pnpm version must match an installed managed version; unsupported versions are not substituted. This iteration publishes pnpm 12.8.1 for Windows x64.

Composer 2.10.3 is an exact checksum-pinned PHAR under Home/tools/composer. It is invoked with the site's managed PHP CLI and managed ini. [Composer's official versioned downloads](https://getcomposer.org/download/) provide the artifact/checksum. This is the Composer foundation, not a full version manager.

Terminal/child PATH starts with selected Node, then relevant managed runtimes/tools, project node_modules/.bin and Windows system directories. It does not inherit global developer-tool PATH. Managed pnpm/composer terminal shims use the selected Node/PHP. PHPRC remains child-local. Managed Node commands also receive PORT/HOST for the reserved loopback port.

Install Dependencies is an explicit action for `pnpm install` or `composer install`, in the project cwd and runtime environment. It keeps logs, owns descendants, has a 10-minute timeout and never replays via service recovery. Tool setup is separately logged. Missing node_modules/vendor and package scripts produce actionable messages. Install scripts may run only as part of the explicitly requested project installation.

## Routing and lifecycle

RouteStrategy metadata selects PHP fastcgi, Node reverse_proxy or Static file_server. Caddy retains local TLS, loopback bindings, generated configuration validation, last-good behavior and site logs. Node gets its own managed port and must have an alive owned listener before a proxy route is admitted. Next receives explicit loopback hostname/port; Vite receives host/port/strictPort. [Vite's documented additional allowed-host environment variable](https://vite.dev/config/server-options.html#server-allowedhosts) permits only the managed site hostname. Caddy's reverse proxy supports WebSocket upgrades.

First Node/worker Start is explicit. Enabled autostart definitions may restart after reopening or Start All. Definitions with autostart=false run only after manual Start and do not start after Stop All/reopen. Site Stop persists the disabled site preference, so Start All does not undo it; explicit site Start reenables the site. Editing a definition disables it and stops the old owned process. Reset removes the local override and restores a disabled portable/detected suggestion.

Laravel PHP web still uses public/ and its normal PHP route. Vite, queue:work and schedule:work are disabled suggestions. Enabled optional jobs are reconciled separately; a failed worker does not remove a valid PHP web route. Custom processes select managed node/pnpm/php/composer, one argument per input line, project-relative cwd, a managed port option and an explicit autostart option.

## UI

Runtime Manager adds Node installation/import/validation/default/removal. Site details show relevant runtime selectors, process state/ports, Start/Stop/Restart, editable custom definitions, dependency/tool actions and portable export. Node-only views hide PHP/MySQL provisioning. Static views have no runtime selector. Site logs combine named relevant service/process/dependency streams without rewriting them into one system log. The Phase 1 setup remains the full-page seven-step wizard.

## Validation and limits

Offline tests cover ordered discovery, package manager conflicts, safe read-only discovery, missing/exact bindings, Node-only/Static dependencies, PHP/Laravel routes, disabled optional jobs, process persistence/authorization revocation, cwd/secret validation, framework flags and child PATH. Native fixture uses official Node, managed pnpm, Caddy local HTTPS and a tiny dependency-free HTTP app; it verifies runtime imports/catalog install, concurrent Node versions, per-site override isolation, ports, restart, stop and reopen.

Verified on 2026-10-04:

- pnpm install --frozen-lockfile, lint, typecheck and build passed; frontend 12 tests in 3 files passed.
- cargo fmt --check and cargo clippy --all-targets -- -D warnings passed.
- cargo test passed 36 tests (27 unit, 1 desktop preference, 8 Phase 2 offline); the 2 external native fixtures are intentionally ignored in the default suite.
- cargo test --features process-fixture --test process_lifecycle passed 5 tests.
- Explicit native_node_pnpm_static_https_and_reopen passed in 9.91 seconds on the final rerun. Its first run installed/imported official runtimes and pinned managed pnpm; reruns reuse those test-only Home installations. It verifies concurrent Node 24.21.0/26.10.0, HTTPS Node/Static routes, exact override isolation, managed ports, restart, failed-first-start cleanup, stop, reopen and persisted site stop behavior.
- git diff --check passed. package.json, pnpm-lock.yaml, Cargo.toml, Cargo.lock and rust-toolchain.toml have no changes.

Native testing uncovered two Windows timing issues, now handled: a bounded retry for the verified staging move after executable validation, and waiting for owned Job descendants before restarting. Site process ports may be reassigned if their former port is unavailable; existing runtime/database port conflict behavior remains unchanged. A failed first Start stops recovery and records Failed while leaving that definition disabled.

 Manual Phase 1 GUI acceptance remains deferred by the task. The release installer was not rebuilt in this iteration. PHP/MySQL native regression requires the original explicit external fixtures; their offline regression remains in the test suite.

Remaining scope limits: npm/yarn automated execution, additional pnpm versions/platform tools and full Composer Manager are future work. Real Next/Vite browser HMR and Laravel hot-file/TLS settings still require project-specific browser acceptance; Laravel plugins may need their own dev-server URL configuration. Generic Node apps must honor PORT/HOST or use an explicit structured command with a managed port. Executing an enabled script or Install Dependencies executes project code under the user's account; structured arguments and Job ownership do not sandbox that application.

No dependency/toolchain versions were upgraded. No automatic commit or push is part of this task.
