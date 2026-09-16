# GHL (Gojo & Haru Language) One-Line Windows PowerShell Installer
# Usage: irm https://raw.githubusercontent.com/ofjorque/ghl/main/install.ps1 | iex

$ErrorActionPreference = "Stop"

$Repo = "ofjorque/ghl"
$InstallDir = if ($env:GHL_INSTALL_DIR) { $env:GHL_INSTALL_DIR } else { "$env:LOCALAPPDATA\ghl\bin" }

Write-Host "(=^･ω･^=) Installing GHL (Gojo & Haru Language) for Windows..." -ForegroundColor Cyan

$Target = "x86_64-pc-windows-msvc"
$ArchiveName = "ghl-${Target}.zip"
$DownloadUrl = "https://github.com/${Repo}/releases/latest/download/${ArchiveName}"

Write-Host "Target platform: $Target" -ForegroundColor Yellow
Write-Host "Downloading from: $DownloadUrl"

if (-not (Test-Path -Path $InstallDir)) {
    New-Item -ItemType Directory -Path $InstallDir -Force | Out-Null
}

$TempDir = Join-Path -Path ([System.IO.Path]::GetTempPath()) -ChildPath "ghl_install_$(Get-Random)"
New-Item -ItemType Directory -Path $TempDir -Force | Out-Null

$ZipFile = Join-Path -Path $TempDir -ChildPath $ArchiveName

try {
    Invoke-WebRequest -Uri $DownloadUrl -OutFile $ZipFile -UseBasicParsing
    Expand-Archive -Path $ZipFile -DestinationPath $InstallDir -Force
} finally {
    Remove-Item -Path $TempDir -Recurse -Force -ErrorAction SilentlyContinue
}

Write-Host "`n(U・ᴥ・U) GHL successfully installed to: $InstallDir\ghl.exe" -ForegroundColor Green

# Add to User PATH if not already present
$UserPath = [Environment]::GetEnvironmentVariable("Path", [EnvironmentVariableTarget]::User)
if ($UserPath -split ";" -notcontains $InstallDir) {
    Write-Host "Adding $InstallDir to User PATH..." -ForegroundColor Yellow
    [Environment]::SetEnvironmentVariable("Path", "$UserPath;$InstallDir", [EnvironmentVariableTarget]::User)
    $env:Path += ";$InstallDir"
    Write-Host "PATH updated for current and future PowerShell sessions." -ForegroundColor Green
}

Write-Host "`nRun 'ghl --help' or 'ghl repl' to start exploring GHL!" -ForegroundColor Cyan
