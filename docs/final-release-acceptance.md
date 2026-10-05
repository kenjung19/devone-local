# Final Windows release acceptance - 2026-10-05

Base: d2bba7ba74ea380d0565175aee97809f639f4774. Version 0.1.0. Tested on this Windows machine, not a clean Windows 10/11 VM. No Phase 4 features, dependency changes or automatic commit/push.

## Baseline and installation

Before changing code, frozen install and pnpm release:windows passed. Baseline root installer: release/DEVONE-Local-0.1.0-Windows-x64-Setup.exe, 8,916,631 bytes, SHA-256 4ca5e8f9dd066ed18080e9d4ccc0a8d21e20d23966218d6da89319bccb547830. Installer and product executable ProductVersion/FileVersion are 0.1.0. Payload audit passed; actual disposable installation contained devone-local.exe, devone-core.exe and uninstall.exe only. Signature status is NotSigned; no signed-release claim.

Silent current-user install into .devone-test/final-installed-20261005/Application passed. Existing DEVONE Local registration was checked absent before installation; the unrelated DEVONE Usage installation was untouched. ARP DisplayVersion was 0.1.0. Installed executable matched built product. Built-release and installed-executable headless smoke both passed Home/SQLite/watcher/activation/single-controller/clean shutdown. Additional installed smoke used a working directory outside source execution and a PATH with only Windows/System32 and Windows, excluding developer environment variables. Normal installed application launch also passed watcher, second-launch activation and abnormal reopen with project persistence. No visual window/focus or tray Quit claim.

Installed devone-core downloaded/installed Caddy 2.11.7 and PHP 8.4.26 from its bundled catalog with integrity/binary validation and Windows-only PATH, requiring no manually authored manifest. This proves packaged core installation, not first-run GUI controls. MySQL installation/initialization GUI and all other installed IPC actions remain unverified. Developer tools were not physically uninstalled from the host.

Silent uninstall of the owned test installation passed: executable and ARP registration removed, disposable Home/SQLite/project files retained, current-user Root certificate list unchanged by uninstall. No user Home was used.

## Executed this session

- Frozen install, lint, typecheck, frontend 25 tests and production build: passed, including the final repeat after adding acceptance tests.
- Final Rust fmt check, all-target clippy with warnings denied and 52 offline tests: passed after all acceptance test edits. The default suite separately lists 11 ignored native tests; no ignored test is counted as an offline pass.
- Process lifecycle 9 tests: passed serialized, including the final repeat in 15.19 seconds, covering cancellation, recovery, port ownership and stale PID protections.
- Copied PHP/Vite discover/start/verified HTTPS/stop smoke: passed in 10.49 seconds.
- Next/Vite/Laravel-Vite HTTPS/HMR protocol regression: passed in 41.25 seconds.
- Node/pnpm/static HTTPS, concurrent Node versions and reopen: passed in 22.62 seconds.
- MySQL 8.4.11 backup/restore/checksum/delete/protected database native test without WordPress: passed in 19.01 seconds.
- All five existing ignored Phase 3 native tests: passed together in 220.91 seconds. Includes Blank PHP, created Next/React-Vite HTTPS, Laravel, WordPress creation with database and Mailpit startup/SMTP/web/stop.
- Strengthened Mailpit SMTP DATA/raw message/stop-start persistence acceptance: passed in 5.49 seconds. Uses the documented [Mailpit raw-message API](https://mailpit.axllent.org/docs/api-v1/).
- Laravel Mailpit opt-in actual message acceptance: passed in 74.88 seconds, including sending a real local message through Laravel and reading it from Mailpit. Existing project .env preservation remains covered. An initial new assertion used an unquoted environment expectation; adjusted to match the product's quoted .env serialization.
- Two real PHP versions 8.4.26 and 8.5.11: passed independently in 8.83 seconds. Simultaneous verified HTTPS responses, version-specific configuration without restarting the other PHP PID, and scoped terminal PATH assertions passed. The test now explicitly trusts only its disposable Home CA; Windows system trust is a separate opt-in test.

## System CA acceptance currently blocked

The disposable CA was added to current-user Root and actual platform trust detection passed. Windows curl then failed with CRYPT_E_NO_REVOCATION_CHECK because the local CA has no online revocation endpoint. The acceptance harness now uses --ssl-no-revoke for Schannel only; certificate and hostname checks remain enabled. No --insecure bypass was added.

Failure cleanup called the product's exact-identity CA removal. Windows opened a Root Certificate Store confirmation. On resuming, the test process was no longer running, but comparison with the recorded current-user Root baseline still showed exactly one extra certificate: 67288AC256C8AC59F42199294828738DDEAF1CF0, Caddy Local Authority - 2026 ECC Root. The user has been asked to remove exactly this test certificate through certmgr.msc. Removal/recreation and complete Windows-system HTTPS acceptance remain unverified. A premature recompilation while the earlier test was running hit Windows LNK1104; that was a harness scheduling error, not a product linker defect.

A subsequent isolated-PHP test initially failed certificate verification when combining its CA with the Windows trust store. Using reqwest tls_certs_only for the disposable Home then passed with signature and hostname checks enabled. This does not prove Windows chain selection with multiple Caddy roots, nor browser trust. Complete CA cleanup and the separate system-client test are still required.

## Unverified acceptance

- Computer Use node_repl failed initialization repeatedly with Windows sandbox helper errors, including reset/retry. No screenshots, rendered layout acceptance or GUI clicks occurred.
- Real one-time UAC/NRPT integration, random .test system lookup, close-to-tray DNS continuity and owned policy removal are unverified. Normal installed startup logs show resolver binding to port 53, but that does not establish Windows DNS policy readiness. An unrelated public hostname resolved normally in this session. No DNS policy was installed/removed.
- Browser visual HTTPS and live rendered HMR, full first-run UI, native SQL chooser, WordPress browser installer, default editor opening, tray actions/window focus, startup login/reboot and layouts at 1280x720 / 1366x768 / 1920x1080 remain manual.
- Installed detection found actual VS Code. Detection/default/custom persistence tests passed, but opening its folder through installed UI was not observed.
- Only MySQL 8.4.11 native fixture was available. Legacy multiversion workflow requires exact PHP 8.3.28/8.5.1 and MySQL 5.7.39/8.4.3 fixtures, which were absent; it was not run and historical passes were not counted.
- Protocol/native core tests do not establish equivalent installed GUI/IPC acceptance.

## Final artifact

Final release build and release smoke passed. release/DEVONE-Local-0.1.0-Windows-x64-Setup.exe is 8,911,645 bytes. SHA-256: 56f881041e200fa7d485159752022afb0e88fa34d7fbdccdc83e282c7d941258. Independently recomputed hash and size match release/release-artifacts.json. Installer ProductVersion/FileVersion are 0.1.0. Signature status remains NotSigned. release/ stays Git-ignored; no installer binary is committed.

The exact final installer was also installed into .devone-test/final-artifact-installed-20261005/Application. Its installed executable passed release smoke and a second smoke with Windows-only PATH and developer variables removed. Silent uninstall removed executable/ARP registration, preserved disposable Home/SQLite and did not change the Root store. Evidence: .devone-test/final-artifact-installed-20261005/final-results.json and sanitized-results.json. The previously recorded normal-mode launch used the baseline installer; no visual acceptance is inferred for either build. Application source/dependency/version files were unchanged between the builds.

## Remaining release gates

Remove the recorded test CA and verify Root baseline restoration; execute corrected Windows-system HTTPS/trust removal/recreation acceptance. Complete installed first-run, real Windows DNS setup/removal and tray lifecycle acceptance, including rendered UI at the requested sizes. GUI automation is unavailable because the Computer Use runtime failed initialization. These are unverified release gates, not demonstrated application defects. MySQL multiversion native acceptance also remains unverified because a second fixture was unavailable. No speculative features are blockers.

Changes in this pass strengthen acceptance tests and record current evidence. No production bug fix, dependency upgrade or version bump was made. Passing automated/native tests does not establish the missing installed desktop/system integration acceptance.

NOT READY FOR DAILY USE
