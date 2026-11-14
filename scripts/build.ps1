<#
.SYNOPSIS
    Builds the Micro-OS kernel and packages the bootable UEFI disk image.
.PARAMETER Release
    Build with optimizations (--release).
#>
param(
    [switch]$Release
)

$ErrorActionPreference = "Stop"
$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Definition
$RootDir = Split-Path -Parent $ScriptDir

Set-Location $RootDir

$ProfileArg = if ($Release) { "--release" } else { "" }

Write-Host "==> Building Micro-OS Kernel & Bootable Image ($ProfileArg)..." -ForegroundColor Cyan
if ($Release) {
    cargo run -p runner -- --build --release
} else {
    cargo run -p runner -- --build
}

if ($LASTEXITCODE -eq 0) {
    Write-Host "`n[OK] Build completed successfully." -ForegroundColor Green
} else {
    Write-Host "`n[ERROR] Build failed." -ForegroundColor Red
    exit 1
}
