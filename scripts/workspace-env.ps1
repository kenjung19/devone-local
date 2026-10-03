# Optional local-cache environment. No global PATH/toolchain defaults are changed.
$workspaceRoot = Split-Path -Parent $PSScriptRoot
$nodeRoot = Join-Path $workspaceRoot '.toolchain/node-v24.21.0-win-x64'
if (Test-Path -LiteralPath (Join-Path $nodeRoot 'node.exe')) { $env:Path = $nodeRoot + ';' + $env:Path }
$rustRoot = Join-Path $workspaceRoot '.toolchain/rustup'
if (Test-Path -LiteralPath (Join-Path $rustRoot 'toolchains/1.99.0-x86_64-pc-windows-msvc/bin/rustc.exe')) { $env:RUSTUP_HOME = $rustRoot }
$cargoCache = Join-Path $workspaceRoot '.toolchain/cargo'
if (Test-Path -LiteralPath $cargoCache) { $env:CARGO_HOME = $cargoCache }
$temporaryRoot = Join-Path $workspaceRoot '.toolchain/tmp'
if (Test-Path -LiteralPath $temporaryRoot) { $env:TEMP = $temporaryRoot; $env:TMP = $temporaryRoot }
