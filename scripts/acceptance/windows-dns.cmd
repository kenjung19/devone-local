@echo off
setlocal
rem Explicit opt-in. Uses the fixed-scope production DNS helper implementation.
rem Refuses existing DEVONE policy and foreign .test policy/listener conflicts.
fltmc >nul 2>&1
if errorlevel 1 (
  echo SKIPPED - NOT ELEVATED: Run this script in CMD as Administrator. No policy was changed.
  exit /b 3
)
pushd "%~dp0..\.."
call scripts\workspace-env.cmd
set "CARGO_BUILD_JOBS=1"
set "DEVONE_ACCEPT_SYSTEM_DNS=1"
if not exist ".devone-test\release-gates-20261005" mkdir ".devone-test\release-gates-20261005"
node scripts/acceptance/log-native.mjs .devone-test/release-gates-20261005/elevated-dns-result.log cargo test --manifest-path src-tauri/Cargo.toml --test release_acceptance disposable_elevated_windows_dns_policy_lifecycle -- --ignored --nocapture --test-threads=1
set "DEVONE_DNS_EXIT=%ERRORLEVEL%"
popd
exit /b %DEVONE_DNS_EXIT%
