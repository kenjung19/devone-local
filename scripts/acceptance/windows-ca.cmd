@echo off
setlocal
pushd "%~dp0..\.."
call scripts\workspace-env.cmd
set "CARGO_BUILD_JOBS=1"
set "DEVONE_ACCEPT_SYSTEM_CA=1"
echo Explicit current-user CA acceptance. Approve only this test's Windows certificate prompt.
echo Exact disposable CA path and SHA-256 are printed before installation.
echo The cleanup guard removes only its exact certificate and verifies Root baseline.
echo Catalog fixtures are downloaded if DEVONE_ACCEPT_PHP_A, DEVONE_ACCEPT_PHP_B or DEVONE_CADDY_SOURCE are unset.
if not exist ".devone-test\release-gates-20261005" mkdir ".devone-test\release-gates-20261005"
node scripts/acceptance/log-native.mjs .devone-test/release-gates-20261005/current-user-ca-result.log cargo test --manifest-path src-tauri/Cargo.toml --test release_acceptance system_ -- --ignored --nocapture --test-threads=1
set "DEVONE_CA_EXIT=%ERRORLEVEL%"
popd
exit /b %DEVONE_CA_EXIT%
