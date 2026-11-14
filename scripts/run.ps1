<#
.SYNOPSIS
    Runs Micro-OS in QEMU.
.PARAMETER Headless
    Runs without a graphical display window (serial stdio only).
.PARAMETER Test
    Runs in automated verification mode and exits upon detecting boot completion.
.PARAMETER Release
    Runs the release build image.
#>
param(
    [switch]$Headless,
    [switch]$Test,
    [switch]$Release
)

$ErrorActionPreference = "Stop"
$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Definition
$RootDir = Split-Path -Parent $ScriptDir

Set-Location $RootDir

$RunnerArgs = @()
if ($Headless) { $RunnerArgs += "--headless" }
if ($Test) { $RunnerArgs += "--test" }
if ($Release) { $RunnerArgs += "--release" }

Write-Host "==> Launching Micro-OS via Runner ($RunnerArgs)..." -ForegroundColor Cyan
cargo run -p runner -- $RunnerArgs
exit $LASTEXITCODE
