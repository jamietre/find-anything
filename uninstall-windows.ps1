# uninstall-windows.ps1
#Requires -RunAsAdministrator

$ErrorActionPreference = "Stop"
$InstallDir = "$env:LOCALAPPDATA\find-anything"

Write-Host "Uninstalling find-anything..." -ForegroundColor Yellow

if (Test-Path "$InstallDir\find-watch.exe") {
    & "$InstallDir\find-watch.exe" uninstall
} else {
    Write-Warning "find-watch.exe not found at $InstallDir; skipping service removal."
}

if (Test-Path $InstallDir) {
    Remove-Item $InstallDir -Recurse -Force
    Write-Host "Removed $InstallDir"
}

Write-Host "Uninstallation complete." -ForegroundColor Green
