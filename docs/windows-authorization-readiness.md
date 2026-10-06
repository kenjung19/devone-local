# Windows authorization and development-release readiness — 2026-10-06

Base: `f7e25c46c030c85c7f616a81174769745dc41afc`. Version 0.1.0.
This report supersedes the rule in [the preceding gate report](windows-release-gates.md)
that an unattended Root approval or missing elevated session necessarily blocks
development-release readiness. It does not claim that optional machine
certification was performed when it was skipped.

## Acceptance policy

**Automated acceptance** verifies product behavior without privileged mutation:
CA generation, exact identity/store lookup, ownership, install/remove command
construction, cancel/failure/retry state, safe recreate, cryptographically verified
local HTTPS, resolver UDP/TCP, foreign listener preservation, exact NRPT payload,
ownership/conflicts, helper argument/result handling, installed desktop workflows
and data/process safety.

**Explicit privileged/interactive acceptance** uses the retained CMD launchers.
They run only on explicit invocation. Windows CA approval and Administrator UAC
are expected installation authorization boundaries. Missing authorization is
reported as skipped/required, never represented as an actual Windows integration
pass. A backend/ownership/cleanup failure remains a failure.

**Manual visual acceptance** includes layouts, tray clicks, foreground focus,
native chooser and editor/browser window observation, and actual login/reboot.
Missing screenshots alone are not product defects.

The normal development release can be ready if automated acceptance and installed
product checks pass, machine operations are fixed in scope and safely reversible,
authorization is requested explicitly, and no known safety/data/runtime defect
remains. The normal application must still report real machine readiness: an
unapproved CA is not trusted, and unconfigured `.test` integration is not ready.
This policy does not change Setup's required readiness checks.

## Production changes

- Preserve numeric bounded-process outcomes so Windows cancellation is
  distinguished without parsing localized output. Existing callers keep their
  checked command behavior and owned-process timeout handling.
- CA installation distinguishes cancelled, approval not completed, actual
  failure and exact trust confirmed. An exact certificate already trusted is
  recognized; a stale DB trusted flag is repaired without reinstalling it.
  Ownership remains recorded before mutation; partial failure remains safely
  removable. A failed attempt with an absent store entry clears stale trusted
  metadata. Install trust returns an actionable exact-state message.
- DNS helper returns separate conflict/not-elevated exit codes; the normal app
  distinguishes UAC cancellation, helper failure, timeout and result-read errors.
  UAC cancellation occurs before the helper launches and reports no changes.
  Timeout does not falsely promise that no change occurred: inspect real state
  and retry/remove the exact owned policy.
- NRPT preflight fails closed on malformed/unreadable namespaces. Existing-key
  ownership is rechecked after create/open before writing, preventing adoption
  of an unowned identifier that appeared after preflight. Namespace remains
  `.test`, server `127.0.0.1`, ConfigOptions 8, Version 2, exact owner marker.
- Automatic resolver port selection reserves TCP first and binds UDP on the
  same port. Windows can otherwise choose a UDP ephemeral port excluded from
  TCP. Fixed port 53 conflict behavior continues to preserve foreign listeners.

No dependency/toolchain/runtime versions or product features were added. No
architecture redesign, permanent app elevation, Windows prompt bypass, insecure
TLS flag or broad certificate/policy deletion was introduced.

## Backend evidence

CA state-machine tests use the production ownership logic with an injected store
boundary. They cover cancel/retry, partial install, DB ownership without a store
entry, exact store entry with stale DB, changed CA file and unrelated same-subject
identity. Removal cannot silently acquire ownership. Public certificate fixtures
contain no private keys and are never installed into Root by automated tests.

The actual CryptoAPI memory-store test exercises the same exact-DER lookup as
production with same-subject certificates and a deliberately changed DER that
retains issuer/serial. It rejects both impostors and removes only the exact
fixture from memory, preserving the other certificate. Install/remove argv tests
require CurrentUser and a full 40-hex Windows certificate identity for deletion;
subject, wildcard and shortened identity are rejected.

Noninteractive native HTTPS uses only the disposable Home's CA. TLS certificate
signature/chain and hostname validation remain enabled; unrelated CA/host are
rejected. Recreate must change fingerprint and remain untrusted. Root baseline
must be unchanged. This is independent of Schannel's optional system-trust test.

Resolver tests exercise both UDP/TCP for `foo.test`, `sub.foo.test`, AAAA with no
managed IPv6 answer, and unrelated/bare domains refused. They verify stop/restart
and conflicts without killing foreign listeners.

NRPT tests write only unique HKCU test keys. They run the production payload writer,
exact/partial ownership predicate and namespace conflict checker. DisplayName-only
partial state is attributable; Name-only is not. Wrong server, config/version,
owner, namespace and malformed types are rejected. The foreign-policy fixture is
preserved when checks reject it. Helper parsing accepts only one fixed setup or
remove argument, never paths/commands or extra argv. Numeric result tests cover
success/retry, cancel, conflict, not elevated, timeout and failure.

## Optional machine certification

From CMD:

```cmd
D:\Project\devone-local\scripts\acceptance\windows-ca.cmd
```

From CMD **Run as administrator**, separately:

```cmd
D:\Project\devone-local\scripts\acceptance\windows-dns.cmd
```

Output/evidence statuses: `PASS` (0), `FAIL` (1),
`INTERACTIVE APPROVAL REQUIRED` (2), `SKIPPED - NOT ELEVATED` (3),
`USER CANCELLED` (4). Approval/cancel classification occurs after exact cleanup.
Only PASS establishes completed optional machine certification. The CA timeout
means authorization did not complete; if already approved, Windows policy must
be investigated instead of assuming missing approval.

Ctrl+C is caught by the native machine guard so bounded work can finish exact
cleanup. Dismiss any pending Windows confirmation and wait. Forced termination,
power loss or closing a console cannot guarantee in-process cleanup; no such
guarantee is claimed. Guards never restore arbitrary machine state: unknown
ownership is refused and reported. Every ordinary pass/error/timeout/cancel path
attempts exact cleanup and verifies its baseline.

In this run, non-elevated DNS invocation reported `SKIPPED - NOT ELEVATED` before
mutation. Interactive CA certification is not invoked unattended. These are
optional certification statuses, not known product defects.

## Current verification and artifact

Current-run frozen install/lint/typecheck, 47 frontend tests, production frontend
build, Rust fmt, normal/instrumented all-target Clippy and **64 default Rust
tests** passed. The default suite separately lists 15 ignored native tests; they
are not included in that pass count. **Nine process lifecycle tests passed**
(8.83 s), including numeric bounded command outcomes and owned child cleanup.

Native two-PHP/TLS/recreate plus port-53 conflict tests passed (32.25 s), and
MySQL 8.4.11/9.7.1 isolation and binding acceptance passed (131.44 s) after shared
process changes. Node/pnpm/static passed with a fresh Home (237.26 s). The first
attempt reused an old fixture's stale site records and failed before the native
workflow; that result is retained and is not counted as a passing run.
HMR Next/Vite/Laravel-Vite passed the cached-fixture repeat (171.18 s), with
verified HTTPS and protocol events. Phase 3 blank PHP and created Next/React
passed. The combined invocation subsequently hit MySQL timeout and Composer/
Mailpit download errors during a long execution-host pause. Only those three
cases were repeated after the host resumed; all passed (228.36 s), including
backup/restore/delete protection, WordPress creation, real Laravel SMTP and
Mailpit raw-message stop/start persistence. The earlier failures are retained,
not counted as passes. Copied PHP/Vite passed (14.73 s) after correcting a harness
invocation that initially omitted its required fixture Home.

Current installed Tauri IPC acceptance passed in
`installed-ipc-1sAb2r/results.json`: real Setup readiness/skip/reopen, catalog
runtime installation, discovery/override/start/stop, DB initialize/provision,
selected-PHP tool action, static creation, Close/hide with watcher continuity,
second-instance activation/show/focus requests, four independent cold-startup
combinations and exact Quit/service/resolver/activation cleanup. The installed
binary was built from current production fixes with only the explicit acceptance
feature added. Its instrumented installer hash was
`325250d4eda743a0b87bb081025f389aea8055324c5c1a35bd536a2d6471a286`;
that is not the normal distributable installer. The disposable instrumented
application was uninstalled after acceptance, preserving its Home/evidence.
Normal release packaging and both built/installed smoke passed. Exact artifact:

- `release/DEVONE-Local-0.1.0-Windows-x64-Setup.exe`
- Version 0.1.0; installed ARP DisplayVersion verified
- **8,920,256 bytes**
- SHA-256 **`2bc123c14ba7ef4b519bc9527fdf12aa5854a5b9b87e969bb04c1bdbeea0fc89`**
- **NotSigned**: no embedded PE certificate table
- Size/hash independently matched `release/release-artifacts.json`
- Normal installed payload only `devone-local.exe`, `devone-core.exe`, `uninstall.exe`;
  payload audit passed and no acceptance transport marker was present
- Installed smoke used Windows-only PATH outside source cwd, verified Home,
  SQLite/watcher/activation/shutdown and removed its activation record
- Exact silent uninstall removed the owned disposable installation/ARP entry,
  preserved its Home and left Root baseline unchanged

Evidence: `final-install-eqnmn2z5/results.json`. The current packaged production
core helper was additionally invoked for both fixed setup/remove arguments with
a verified non-elevated token: both returned exit 3/NOT ELEVATED and the exact
production policy state remained unchanged (`packaged-helper-non-elevated.json`).
This is a helper-boundary check, not an elevated routing certification.

## Requested final report

| # | Item | Result |
|---|---|---|
| 1 | Production changes | Numeric authorization/helper outcomes, stale CA metadata repair, NRPT fail-closed/raced-key checks, paired ephemeral DNS ports; scope above |
| 2 | CA backend | Exact commands/DER/ownership, cancel/retry/partial/stale-state and cleanup refusal tests passed |
| 3 | Interactive CA | Optional certification; not invoked unattended in this run; Windows approval remains an installation step |
| 4 | TLS cryptography | Real fresh-Home HTTPS accepted with only its CA; wrong CA/host rejected, recreate changed identity without trust; Root baseline unchanged |
| 5 | Resolver | UDP/TCP A, subdomain, AAAA no managed IPv6, unrelated refusal, stop/restart and foreign conflicts passed |
| 6 | NRPT helper/ownership | Exact payload, partial/foreign/malformed ownership, namespace conflicts, raced identifier and fixed argv/result tests passed |
| 7 | Elevated DNS | Optional certification; non-elevated CMD returned SKIPPED before mutation; no actual system-routing pass claimed |
| 8 | Cancellation | Numeric CA/UAC cancel handling, actionable retry, partial-state ownership and exact cleanup safety passed; OS dialog clicking is manual |
| 9 | Installed desktop | Current installed Tauri IPC passed all required actions, Close/watcher, Quit, activation and four startup combinations |
| 10 | Runtime regressions | Two PHP, Node/pnpm/static, HMR, all five Phase 3 cases across recorded runs, real SMTP/persistence and copied PHP/Vite passed |
| 11 | MySQL multiversion | Exact 8.4.11/9.7.1 data/process/port and per-site binding isolation passed after shared process changes |
| 12 | Automated verification | Frozen install/lint/typecheck/build, frontend 47, Rust fmt/Clippy/default 64, process lifecycle nine passed |
| 13 | Installer | `release/DEVONE-Local-0.1.0-Windows-x64-Setup.exe`; normal build, payload audit and built/installed sanitized smoke passed |
| 14 | Size | 8,920,256 bytes |
| 15 | SHA-256 | `2bc123c14ba7ef4b519bc9527fdf12aa5854a5b9b87e969bb04c1bdbeea0fc89` |
| 16 | Signing | NotSigned; embedded PE signature absent |
| 17 | Machine commands | `scripts\acceptance\windows-ca.cmd`, `scripts\acceptance\windows-dns.cmd`; optional explicit certification, never unattended mutation |
| 18 | Manual visual checks | Layout/overflow at requested sizes, tray/menu/focus, chooser/editor/browser windows and actual login/reboot |
| 19 | Known defects | No demonstrated unresolved safety/data/runtime defect; current automated/native/installed/packaging checks passed |
| 20 | Verdict | Development-release daily-use criteria met; absent unattended Windows approval alone is not a blocker |

Windows status semantics are based on numeric API results, including
[ERROR_CANCELLED](https://learn.microsoft.com/en-us/windows/win32/debug/system-error-codes--1000-1299-),
[ShellExecuteExW](https://learn.microsoft.com/en-us/windows/win32/api/shellapi/nf-shellapi-shellexecuteexw)
and [WaitForSingleObject](https://learn.microsoft.com/en-us/windows/win32/api/synchapi/nf-synchapi-waitforsingleobject).
No localized output parsing is used to decide that an operation was cancelled.

The fresh HMR attempt stopped during pnpm tool installation on a bounded network
timeout, before protocol assertions. Its verified cached pnpm/Composer/PHP
fixtures were copied into the disposable Home and the protocol test passed its
repeat. This failed fixture preparation is retained in the log, not counted as a
passing HMR run or evidence of a Windows authorization defect.

Evidence is under `.devone-test/release-gates-20261005/`: this path is retained
from the previous task, while the current run uses the `approval-*` logs.
Do not use the previous build's hash or historical regression counts as evidence
for this build.

Normal Windows Setup still requires the user to authorize the exact CA trust and
isolated elevated DNS helper. Optional real system-trust and elevated system-DNS
certification remain explicitly unperformed in this run; visual-only checks
remain manual. This is separate from the completed automatic product acceptance.
There is no known concrete blocker under the requested development-release
policy. No automatic commit/push was performed; installer output stays ignored.

READY FOR DAILY USE
