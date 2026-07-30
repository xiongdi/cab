# CAB one-line installer for Windows (cab + dashboard UI).
#
#   irm https://raw.githubusercontent.com/xiongdi/cab/main/scripts/install.ps1 | iex
#   irm https://xiongdi.github.io/cab/install.ps1 | iex
#
# Options:
#   -Version <ver>       Install a specific version (e.g. 0.9.0 or v0.9.0)
#   -InstallRoot <path>  Install root (default: %USERPROFILE%\.cab)
#   -NoModifyPath        Do not add %USERPROFILE%\.cab\bin to user PATH
#   -NoService           Skip `cab service install`
#Requires -Version 5.1
[CmdletBinding()]
param(
    [string]$Version = $env:CAB_VERSION,
    [string]$InstallRoot = $(if ($env:CAB_INSTALL_ROOT) { $env:CAB_INSTALL_ROOT } else { Join-Path $env:USERPROFILE '.cab' }),
    [switch]$NoModifyPath,
    [switch]$NoService
)

$ErrorActionPreference = 'Stop'
$Repo = if ($env:CAB_REPO) { $env:CAB_REPO } else { 'xiongdi/cab' }

function Write-Muted([string]$Text) { Write-Host $Text -ForegroundColor DarkGray }
function Write-Ok([string]$Text) { Write-Host $Text -ForegroundColor Green }
function Write-Err([string]$Text) { Write-Host $Text -ForegroundColor Red }

# Native arch (handles ARM64 Windows and x64).
$arch = switch ([System.Runtime.InteropServices.RuntimeInformation]::OSArchitecture.ToString()) {
    'X64' { 'x64' }
    'Arm64' { 'arm64' }
    default {
        Write-Err "Unsupported processor architecture: $([System.Runtime.InteropServices.RuntimeInformation]::OSArchitecture)"
        exit 1
    }
}

$os = 'windows'
$asset = "cab-${os}-${arch}.zip"

$api = "https://api.github.com/repos/$Repo/releases"
if ([string]::IsNullOrWhiteSpace($Version)) {
    Write-Muted 'Fetching latest release…'
    $release = Invoke-RestMethod -Uri "$api/latest" -Headers @{ 'User-Agent' = 'cab-installer' }
} else {
    $tag = $Version.TrimStart('v')
    $tag = "v$tag"
    Write-Muted "Fetching release $tag…"
    try {
        $release = Invoke-RestMethod -Uri "$api/tags/$tag" -Headers @{ 'User-Agent' = 'cab-installer' }
    } catch {
        Write-Err "Release $tag not found"
        exit 1
    }
}

$tagName = $release.tag_name
$ver = $tagName.TrimStart('v')
$url = "https://github.com/$Repo/releases/download/$tagName/$asset"

Write-Muted "Installing CAB $ver ($os-$arch)"
Write-Muted "Asset: $asset"

$binDir = Join-Path $InstallRoot 'bin'
$uiDir = Join-Path $InstallRoot 'ui'
$tmp = Join-Path ([System.IO.Path]::GetTempPath()) ("cab-install-" + [guid]::NewGuid().ToString('n'))
New-Item -ItemType Directory -Force -Path $tmp, $binDir | Out-Null

$archive = Join-Path $tmp $asset
try {
    Write-Muted "Downloading $url …"
    Invoke-WebRequest -Uri $url -OutFile $archive -UseBasicParsing
    if ((Get-Item $archive).Length -eq 0) {
        throw "Downloaded archive is empty"
    }

    $extract = Join-Path $tmp 'extract'
    Expand-Archive -Path $archive -DestinationPath $extract -Force

    $payload = $extract
    $cabSrc = Join-Path $payload 'cab.exe'
    if (-not (Test-Path $cabSrc)) {
        $nested = Get-ChildItem -Path $extract -Directory | Where-Object {
            Test-Path (Join-Path $_.FullName 'cab.exe')
        } | Select-Object -First 1
        if ($nested) {
            $payload = $nested.FullName
            $cabSrc = Join-Path $payload 'cab.exe'
        }
    }
    if (-not (Test-Path $cabSrc)) {
        throw 'Archive missing cab.exe'
    }

    $cabDst = Join-Path $binDir 'cab.exe'

    # Stop any running CAB service so cab.exe is not locked.
    Write-Muted 'Stopping existing CAB service…'
    schtasks /End /TN "CAB\cab-srv" /F 2>$null
    Start-Sleep -Milliseconds 300
    taskkill /F /IM cab.exe 2>$null
    Start-Sleep -Milliseconds 500

    try {
        Copy-Item -Path $cabSrc -Destination $cabDst -Force
    } catch {
        # Windows may still refuse the copy (AV scan, etc.) — fall back to cmd copy.
        Write-Muted 'Copy-Item blocked, falling back to cmd /c copy /Y…'
        cmd /c copy /Y $cabSrc $cabDst | Out-Null
        if ($LASTEXITCODE -ne 0) {
            throw "Failed to replace $cabDst : $_"
        }
    }

    $uiSrc = Join-Path $payload 'ui'
    if (Test-Path $uiSrc) {
        if (Test-Path $uiDir) { Remove-Item -Recurse -Force $uiDir }
        Copy-Item -Path $uiSrc -Destination $uiDir -Recurse -Force
    }

    $meta = @{
        version      = $ver
        os           = $os
        arch         = $arch
        bin_dir      = $binDir
        ui_dir       = $uiDir
        installed_at = (Get-Date).ToUniversalTime().ToString('yyyy-MM-ddTHH:mm:ssZ')
    } | ConvertTo-Json
    Set-Content -Path (Join-Path $InstallRoot 'install.json') -Value $meta -Encoding UTF8

    if (-not $NoModifyPath) {
        $userPath = [Environment]::GetEnvironmentVariable('Path', 'User')
        if ($userPath -notlike "*$binDir*") {
            $newPath = if ([string]::IsNullOrWhiteSpace($userPath)) { $binDir } else { "$binDir;$userPath" }
            [Environment]::SetEnvironmentVariable('Path', $newPath, 'User')
            $env:Path = "$binDir;$env:Path"
            Write-Muted "Added to user PATH: $binDir"
        } else {
            $env:Path = "$binDir;$env:Path"
        }
    } else {
        $env:Path = "$binDir;$env:Path"
    }

    Write-Host ''
    Write-Ok "CAB $ver installed to $binDir"
    Write-Muted "Binary: cab.exe"
    if (Test-Path $uiDir) { Write-Muted "UI: $uiDir" }

    if (-not $NoService) {
        Write-Muted 'Installing user service…'
        & $cabDst service install --scope user
        if ($LASTEXITCODE -eq 0) {
            & $cabDst start
            Write-Muted 'Gateway: http://127.0.0.1:3125'
        } else {
            Write-Muted 'Service install skipped/failed — run later:'
            Write-Host '  cab service install --scope user'
            Write-Host '  cab start'
        }
    }

    Write-Host ''
    Write-Muted 'Next:'
    Write-Host '  cab status'
    Write-Host '  cab gui                 # open dashboard in browser'
    Write-Host '  cab update              # upgrade to latest release'
    Write-Host '  Docs: https://xiongdi.github.io/cab/'
    Write-Host ''
    if ($NoModifyPath) {
        Write-Muted "Add to PATH: $binDir"
    } else {
        Write-Muted 'Open a new terminal if `cab` is not found yet.'
    }
} catch {
    Write-Err $_.Exception.Message
    exit 1
} finally {
    if (Test-Path $tmp) { Remove-Item -Recurse -Force $tmp -ErrorAction SilentlyContinue }
}
