@echo off
rem Optional workspace caches only; does not modify the system PATH or defaults.
for %%I in ("%~dp0..") do set "DEVONE_WORKSPACE=%%~fI"
if exist "%DEVONE_WORKSPACE%\.toolchain\node-v24.21.0-win-x64\node.exe" set "PATH=%DEVONE_WORKSPACE%\.toolchain\node-v24.21.0-win-x64;%PATH%"
if exist "%DEVONE_WORKSPACE%\.toolchain\rustup\toolchains\1.99.0-x86_64-pc-windows-msvc\bin\rustc.exe" set "RUSTUP_HOME=%DEVONE_WORKSPACE%\.toolchain\rustup"
if exist "%DEVONE_WORKSPACE%\.toolchain\cargo" set "CARGO_HOME=%DEVONE_WORKSPACE%\.toolchain\cargo"
if exist "%DEVONE_WORKSPACE%\.toolchain\tmp" set "TEMP=%DEVONE_WORKSPACE%\.toolchain\tmp"
if exist "%DEVONE_WORKSPACE%\.toolchain\tmp" set "TMP=%DEVONE_WORKSPACE%\.toolchain\tmp"
