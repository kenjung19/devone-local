# Windows release gates — 2026-10-06

This report continues `main` at `01ba1ae618846dccb6e0b8bbb0e0d93f291d65a9`
(`test: strengthen final Windows release acceptance`). Version remains 0.1.0.
No Phase 4 features, dependency/toolchain version upgrades, automatic commit or
push. Evidence lives in `.devone-test/release-gates-20261005/`; dates in fixture
paths reflect when this acceptance run started.

This supersedes the pending gates in [the previous acceptance report](final-release-acceptance.md).
The results below are from this development run; an old installer is not
evidence for the new production fixes.

## Production fixes

- CA removal uses system `certutil -user -delstore Root` with the full exact
  SHA-1 certificate identity extracted from the owned CA, bounded execution and
  an actual absence check. It never matches by subject. The application checks
  its recorded SHA-256 CA-file ownership before removing trust.
- CA trust installation is bounded; its ownership record is written before the
  Windows mutation so partial installation remains removable. A changed CA
  file cannot overwrite another recorded CA identity. Trust lookup also compares
  exact encoded certificate bytes, not only issuer/serial identity.
- NRPT ownership rejects changed namespace, DNS server, configuration/version
  values and invalid registry types. Partial genuinely owned helper writes can
  be cleaned, while foreign policy cannot be adopted or overwritten.
- First-run MySQL readiness requires an initialized instance with its data
  directory. Installing binaries alone previously marked it ready too early.
- SQL selection uses the native Win32 chooser, with no PowerShell requirement.
  Chooser/execution share validation; `.SQL` works on Windows, unsupported,
  missing, relative and linked sources are rejected, cancellation returns no
  selection, and a user-selected source outside Home is preserved.
- Actual Tauri Quit could skip stack destructors and leave its activation
  endpoint record behind. The Exit event now explicitly shuts down that owner's
  activation listener and removes its record before process termination.

## Exact system cleanup and local TLS

The previously recorded disposable root
`67288AC256C8AC59F42199294828738DDEAF1CF0` was present at the start and removed
by its full exact thumbprint. Before/after current-user Root evidence confirms
only that entry disappeared; unrelated certificates were unchanged. Evidence:
`root-before-cleanup.json`, `root-after-cleanup.json`, `cleanup-out.txt`.

The reusable machine guard captures a private immutable copy of each test's
exact CA before trust installation, rejects an already trusted CA, stops its own
application resources, removes only captured owned certificates/policy, and
checks the Root baseline. Cleanup failure fails acceptance rather than restoring
arbitrary machine state. Its controlled-panic test must prove actual installation
was reached; a trust timeout cannot be misreported as panic-after-trust success.

Earlier in this development run, system HTTPS/trust/remove/recreate and the
controlled-panic cleanup passed. The latest full native attempt timed out waiting
for Windows CA installation confirmation. Cleanup returned the Root baseline
without a cleanup failure. This latest system-CA run is **not a pass**. Evidence:
`ca-timeout-regression.log`. Re-run the explicit interactive CMD workflow before
closing this current-code gate.

The local offline CA has no online revocation endpoint. The Schannel acceptance
client uses `--ssl-no-revoke` only for that test; it still validates current-user
root trust, certificate chain/signature and hostname, and must fail after root
removal. No product `--insecure` behavior or global TLS verification disable was
added. See [curl's option](https://curl.se/docs/manpage.html#--ssl-no-revoke) and
[Caddy local HTTPS](https://caddyserver.com/docs/automatic-https#local-https).

## CMD workflows and pending machine integration

Use [the acceptance instructions](../scripts/acceptance/README.md).

```cmd
D:\Project\devone-local\scripts\acceptance\windows-dns.cmd
```

Run that command from **CMD as Administrator**. The normal session's elevation
check failed before modifying NRPT. At the latest inspection, the production
policy was absent and no elevated-result log had been recorded. Actual Windows
system DNS routing and exact policy removal remain unverified until that run
passes. This is a concrete machine-integration gate, not a missing screenshot.

For current-user CA acceptance, use CMD:

```cmd
D:\Project\devone-local\scripts\acceptance\windows-ca.cmd
```

Only approve this test's printed disposable CA in Windows. Run it after other
native acceptance finishes; it requires exclusive ports 80/443. It obtains exact
catalog fixtures if optional local fixture paths are absent.
Output remains visible and is saved to
`current-user-ca-result.log` in the evidence directory. All noninteractive/native
tests described below have now finished, so their listeners no longer conflict
with that invocation. No test certutil/Caddy process remained at final inspection.

## Installed desktop boundary

An actual NSIS-installed build with the explicit `release-acceptance` feature ran
the real Tauri app, watcher, resolver and controller under Windows-only PATH. The
token-scoped client calls the same shared application action implementation as
GUI IPC; tray Quit and window Close use production event/command handlers.
Machine policy/CA/credential-reveal actions are excluded from this transport.

Both successful installed runs verified snapshot, catalog runtime install,
discovery/override/start/stop, MySQL initialize/provision, selected-PHP Composer
action, static project creation, settings and setup reject/skip/reopen. They
verified actual Close hides the window while watcher/controller/resolver remain
alive; second launch activates the same controller and show/focus APIs succeed;
Quit stops owned services/resolver/controller and removes the activation record.

All four Windows startup OFF/ON × environment autostart OFF/ON combinations
passed exact HKCU Run registration and cold reopen checks. The two settings
remain independent and exact test-owned registration cleanup preserved baseline.
Detected VS Code was launched through the backend with structured project argv;
the command-construction test verifies no injected database credential arguments
or environment. Visual editor/window focus observation is not claimed.

Evidence: `installed-ipc-F20PM3/results.json` and
`installed-ipc-Z4C93s/results.json`; `instrumented-install/installation.json`.
The disposable installation was uninstalled before normal release packaging.
Only runtime binary copies in those completed Homes were reclaimed for disk
space; config, databases, projects and acceptance evidence remain.

This test feature is excluded from the normal final installer. Instrumented
installer SHA-256 `1e79665be01046976e5480c64b1989ffec131388db46c49ef62ee1d5019099b7`
is **not** the distributable release hash.

## Runtime catalog and MySQL isolation

The catalog adds exact Windows x64 MySQL 9.7.1 alongside 8.4.11. Official archive:
`https://cdn.mysql.com/Downloads/MySQL-9.7/mysql-9.7.1-winx64.zip`.
The bytes were authenticated against Oracle's detached signature and official
2025 build key before computing SHA-256
`d839045f3cc86a7c5791afc2d8e2a24c0ff6ae4f91a97b2b212c9e032d04d6ea`.
The signing fingerprint was
`BCA43417C3B485DD128EC6D4B7B3B788A8D3785C`; official MD5 also matched
`8778341c62eb2ab1a95b1f22bee70f9e`. Oracle did not supply a SHA-256 sidecar:
this SHA-256 was computed from the authenticated official artifact. Evidence:
`mysql-authority/verification.json`, `signature-verification.txt`.
See [Oracle's verification instructions](https://dev.mysql.com/doc/refman/9.7/en/checking-gpg-signature.html)
and [Windows package documentation](https://dev.mysql.com/doc/refman/9.7/en/windows-choosing-package.html).

Current native acceptance passed concurrent 8.4.11/9.7.1 binaries, independent
ports/data directories, real protocol health and contents, stop/restart
isolation, cold reopen and persisted credentials/data. Two provisioned PHP sites
retain their own runtime/port bindings when the global MySQL default changes;
existing `.env` is preserved. There is no engine-major data-directory reuse or
automatic migration. Latest run: 37.13 seconds, `native-regressions.log`.

## Validation status

Frozen pnpm install, lint, typecheck, frontend 47 tests, build, Rust fmt, all-target
Clippy for normal and instrumented features, default Rust suite, and nine process
lifecycle tests passed in the full verification run. Native port-53 foreign UDP
and TCP conflicts/retry/restart and two real PHP versions passed the latest
noninteractive repeat. Node/pnpm/static passed in a fresh Node-only Home.
The final repeat of normal/instrumented all-target Clippy, fmt and default Rust
tests also passed after extracting the verified-HTTPS readiness helper. HMR
passed again in 57.44 seconds with that helper. The previous fresh-Home attempt
had sent its first TLS request while Caddy was still asynchronously issuing its
local certificate: Caddy's log recorded successful issuance about 35 ms after
the listener became ready. The helper polls verified HTTPS with a deadline and
never disables verification or retries HTTP errors.

Phase 3's blank PHP, MySQL backup/restore/delete protection plus WordPress
creation, Laravel creation with real Mailpit SMTP message, and Mailpit raw-message
stop/start persistence all passed. Created Next/React passed the focused repeat
in 65.66 seconds; copied PHP/Vite passed in 8.39 seconds using the same helper.
All five Phase 3 native cases therefore have current passing results; the earlier
combined invocation's starter failure is retained in the evidence, not counted
as a passing combined suite. No production source changed between these native
runs and final normal packaging.

Final validation: 47 frontend tests, **56 default Rust tests** (15 ignored native
cases reported separately), and **nine process lifecycle tests** passed. Native
results are counted only from their explicit executions. Evidence:
`final-verification.log`, `final-rust.log`, `native-regressions.log`,
`remaining-native.log`, `protocol-phase3.log`, `focused-native.log`.

Structural React tests cover each full-page Setup step, required readiness
controls, Sites empty/Ready/Needs Setup/Error, New Project, Runtimes, Database,
Tools, Settings and error/details. SSR markup does not measure layout/overflow
at 1280×720, 1366×768 or 1920×1080 and does not establish visual approval.
SQL chooser boundary and structured editor launch tests passed.

Manual-only items: rendered layouts, visual tray/menu clicks, actual foreground
focus, native chooser visuals, actual Windows login/reboot, browser/editor window
observation. Their absence alone does not determine the readiness verdict.

## Final normal artifact

`pnpm release:windows` passed after all production fixes. Normal release and the
exact installed executable both passed Home/SQLite/watcher/second-instance/clean
shutdown smoke. The installed smoke ran outside the source cwd with Windows-only
PATH and developer environment variables removed. The installed payload was only
`devone-local.exe`, `devone-core.exe` and `uninstall.exe`; normal binary inspection
confirmed the acceptance transport was excluded. Silent uninstall removed the
exact disposable application/ARP entry, preserved its Home and restored the
unchanged Root baseline. Evidence: `final-install-_7ktuuxv/results.json`.

- Installer: `release/DEVONE-Local-0.1.0-Windows-x64-Setup.exe`
- Metadata: `release/release-artifacts.json`
- Version: 0.1.0; installed ARP DisplayVersion verified
- Size: **8,920,468 bytes**
- SHA-256: **`82230ae9b87611000dfdc0d1aaf843b19a3bf10b03206f5a90dfb2745ec53163`**
- Signature: **NotSigned**, embedded PE certificate table absent
- Payload audit passed; installer/metadata size and SHA-256 independently matched
- Installer remains Git-ignored and is not committed

## Requested acceptance summary

| # | Item | Current result |
|---|---|---|
| 1 | Base commit | `01ba1ae618846dccb6e0b8bbb0e0d93f291d65a9` |
| 2 | Production bugs found | Fragile root removal/partial trust ownership, altered NRPT ownership, premature MySQL readiness, SQL chooser/extension boundary, Tauri activation-record leak on Quit |
| 3 | Production bugs fixed | Exact bounded CA operations/DER lookup, strict NRPT fields, initialized MySQL readiness, native chooser/shared SQL validation, explicit desktop Exit cleanup |
| 4 | Recorded test CA | Exact thumbprint removed; unrelated roots unchanged |
| 5 | CA trust/remove/recreate | Earlier pass in this run; latest repeat blocked at Windows confirmation timeout, so current gate remains open |
| 6 | Windows system HTTPS | Earlier verified Schannel request with hostname/root checks; latest complete repeat pending interactive CA confirmation |
| 7 | DNS/NRPT | Administrator opt-in CMD test supplied; real system lookup/removal result pending; no policy installed by unelevated attempt |
| 8 | Tray backend lifecycle | Installed Close/hide/watcher continuity and real Quit shutdown passed |
| 9 | Single-instance activation | Same controller PID, activation signal and show/focus API requests passed |
| 10 | Windows startup | Exact HKCU registration and four independent cold startup combinations passed; baseline cleanup passed |
| 11 | MySQL multiversion | Concurrent exact 8.4.11/9.7.1, protocol/data/ports/binaries, isolation and reopen passed |
| 12 | Per-site DB runtime | Two independent bindings and global-default-change protection passed; `.env` retained |
| 13 | Installed IPC | Runtime, site, DB, tool, project and settings actions passed through real installed Tauri boundary |
| 14 | First-run state machine | Real initial readiness, MySQL initialize readiness, incomplete Finish rejection, explicit skip/reopen passed; machine-ready complete path pending CA/DNS |
| 15 | UI structure | 47 frontend tests passed; SSR layout measurements/visual approval not claimed |
| 16 | File chooser boundary | Cancel, external `.SQL`, unsupported/missing/relative/link protection and source preservation passed |
| 17 | Editor launch | Detected VS Code backend spawn and structured argv/no injected DB credentials checks passed; visual editor observation manual |
| 18 | Regressions | Frozen install/lint/typecheck/build, frontend 47, Rust fmt/Clippy/default 56, process nine, native PHP/port53/MySQL/Node/HMR, five Phase 3 cases, SMTP persistence/real Laravel message, copied PHP/Vite and release/installed smoke passed; latest system-CA repeat and elevated DNS pending |
| 19 | Installer path | `release/DEVONE-Local-0.1.0-Windows-x64-Setup.exe` |
| 20 | Installer size | 8,920,468 bytes |
| 21 | SHA-256 | `82230ae9b87611000dfdc0d1aaf843b19a3bf10b03206f5a90dfb2745ec53163` |
| 22 | Signing | NotSigned (embedded signature absent) |
| 23 | Known remaining product defects | No additional demonstrated unresolved production defect; pending machine integration is not counted as passed |
| 24 | Manual visual-only checks | Layout/overflow at requested sizes, visual tray/focus, chooser/editor/browser windows and actual login/reboot |
| 25 | Concrete daily-use gates | Latest complete system-CA acceptance and real elevated NRPT ownership/system lookup/removal still need passing current runs |

No new product feature or missing screenshot is a blocker. The remaining gates
are current machine-integration results: Windows CA confirmation must complete
and guarded trust/remove/recreate/system HTTPS must pass; the Administrator CMD
NRPT test must demonstrate actual Windows routing, ownership and removal.
There is no automatic commit/push in this task.

NOT READY FOR DAILY USE
