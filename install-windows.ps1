# install-windows.ps1
# Downloads the latest find-anything release for Windows and installs it.
#Requires -RunAsAdministrator

$ErrorActionPreference = "Stop"
$InstallDir = "$env:LOCALAPPDATA\find-anything"
$ConfigPath = "$InstallDir\client.toml"
$ServiceName = "FindAnythingWatcher"

Write-Host "find-anything Windows installer" -ForegroundColor Cyan
Write-Host ""

# Detect latest release from GitHub API
$Release = Invoke-RestMethod "https://api.github.com/repos/findanything/find-anything/releases/latest"
$Asset = $Release.assets | Where-Object { $_.name -like "*windows-x86_64*.zip" } | Select-Object -First 1
if (-not $Asset) { throw "Could not find Windows release asset" }

$ZipPath = "$env:TEMP\find-anything-windows.zip"
Write-Host "Downloading $($Asset.name)..." -ForegroundColor Yellow
Invoke-WebRequest -Uri $Asset.browser_download_url -OutFile $ZipPath

Write-Host "Extracting to $InstallDir..."
if (Test-Path $InstallDir) { Remove-Item $InstallDir -Recurse -Force }
New-Item -ItemType Directory -Path $InstallDir | Out-Null
Expand-Archive -Path $ZipPath -DestinationPath "$env:TEMP\find-anything-extract"
$ExtractedDir = Get-ChildItem "$env:TEMP\find-anything-extract" | Select-Object -First 1
Move-Item "$($ExtractedDir.FullName)\*" $InstallDir

# Create config template if not present
if (-not (Test-Path $ConfigPath)) {
    @"
[server]
url   = "http://localhost:8080"
token = "CHANGE_ME"

[[sources]]
name  = "home"
paths = ["$env:USERPROFILE"]
"@ | Set-Content $ConfigPath
    Write-Host ""
    Write-Host "Opening config file for editing. Set 'url' and 'token' to match your server." -ForegroundColor Yellow
    Start-Process notepad.exe $ConfigPath -Wait
}

# Register service
Write-Host "Installing Windows service..." -ForegroundColor Yellow
& "$InstallDir\find-watch.exe" install --config $ConfigPath

Write-Host ""
Write-Host "Installation complete!" -ForegroundColor Green
Write-Host "  Binaries: $InstallDir"
Write-Host "  Config:   $ConfigPath"
Write-Host "  Service:  $ServiceName"
Write-Host ""
Write-Host "Start the service: sc start $ServiceName"
Write-Host "Run a full scan:   $InstallDir\find-scan.exe --config $ConfigPath"
