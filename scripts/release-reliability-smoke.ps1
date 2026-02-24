[CmdletBinding()]
param(
    [string]$AssetsDir = $(if ($env:ONESHIM_SMOKE_ASSETS_DIR) { $env:ONESHIM_SMOKE_ASSETS_DIR } else { "dist" }),
    [string]$InstallScript = $(if ($env:ONESHIM_INSTALL_SCRIPT) { $env:ONESHIM_INSTALL_SCRIPT } else { "scripts/install.ps1" }),
    [string]$AssetName = $(if ($env:ONESHIM_SMOKE_ASSET_NAME) { $env:ONESHIM_SMOKE_ASSET_NAME } else { "oneshim-windows-x64.zip" }),
    [string]$InstallDir = $(if ($env:ONESHIM_SMOKE_INSTALL_DIR) { $env:ONESHIM_SMOKE_INSTALL_DIR } else { "" }),
    [string]$Host = $(if ($env:ONESHIM_SMOKE_HOST) { $env:ONESHIM_SMOKE_HOST } else { "127.0.0.1" }),
    [int]$Port = $(if ($env:ONESHIM_SMOKE_PORT) { [int]$env:ONESHIM_SMOKE_PORT } else { 18091 }),
    [switch]$SkipUpdaterTests
)

$ErrorActionPreference = "Stop"

function Write-Info {
    param([string]$Message)
    Write-Host "[SMOKE] $Message"
}

function Throw-IfMissing {
    param(
        [string]$Path,
        [string]$Label
    )
    if (!(Test-Path $Path)) {
        throw "$Label not found: $Path"
    }
}

Throw-IfMissing -Path $AssetsDir -Label "Asset directory"
Throw-IfMissing -Path $InstallScript -Label "Install script"

$artifactPath = Join-Path $AssetsDir $AssetName
$checksumPath = "$artifactPath.sha256"
Throw-IfMissing -Path $artifactPath -Label "Artifact"
Throw-IfMissing -Path $checksumPath -Label "Checksum"

$pythonCommand = Get-Command python3 -ErrorAction SilentlyContinue
if (-not $pythonCommand) {
    $pythonCommand = Get-Command python -ErrorAction SilentlyContinue
}
if (-not $pythonCommand) {
    throw "Python is required to host local release assets"
}

$tempDir = Join-Path $env:TEMP ("oneshim-release-smoke-" + [Guid]::NewGuid().ToString("N"))
New-Item -ItemType Directory -Path $tempDir -Force | Out-Null
if ([string]::IsNullOrWhiteSpace($InstallDir)) {
    $InstallDir = Join-Path $tempDir "bin"
}

$serverProcess = $null
try {
    Write-Info "Serving assets from $AssetsDir on http://$Host`:$Port"
    $serverProcess = Start-Process `
        -FilePath $pythonCommand.Source `
        -ArgumentList @("-m", "http.server", "$Port", "--bind", "$Host") `
        -WorkingDirectory $AssetsDir `
        -PassThru `
        -WindowStyle Hidden
    Start-Sleep -Seconds 1

    if ($serverProcess.HasExited) {
        throw "Failed to start local HTTP server"
    }

    $baseUrl = "http://$Host`:$Port"
    Write-Info "Running installer against local base URL"
    & powershell `
        -ExecutionPolicy Bypass `
        -File $InstallScript `
        -InstallDir $InstallDir `
        -BaseUrl $baseUrl

    $target = Join-Path $InstallDir "oneshim.exe"
    Throw-IfMissing -Path $target -Label "Installed binary"

    Write-Info "Validating first-run command"
    & $target --version | Out-Null

    if (-not $SkipUpdaterTests) {
        Write-Info "Running updater reliability regression tests"
        & cargo test -p oneshim-app release_reliability_ -- --nocapture
    }

    Write-Info "Release reliability smoke completed"
} finally {
    if ($serverProcess -and -not $serverProcess.HasExited) {
        Stop-Process -Id $serverProcess.Id -Force -ErrorAction SilentlyContinue
    }
    Remove-Item -Path $tempDir -Recurse -Force -ErrorAction SilentlyContinue
}
