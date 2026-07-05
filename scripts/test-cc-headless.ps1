# Real Claude Code headless test against the global CAB gateway (localhost:3125).
# Does NOT start cab-server — dev:server must already be running (see AGENTS.md).
param(
    [string]$Prompt = "Reply with exactly: CAB ok",
    [string]$Model = "claude/cab/auto",
    [int]$TimeoutSec = 180
)

$ErrorActionPreference = "Stop"
$GatewayPort = 3125
$Base = "http://127.0.0.1:${GatewayPort}"

function Test-PortListening {
    param([int]$Port)
    return $null -ne (netstat -ano | Select-String ":$Port\s+.*LISTENING")
}

if (-not (Test-PortListening $GatewayPort)) {
    Write-Host "Port $GatewayPort not listening — kill stale holders then start: npm run dev:server"
    & "$PSScriptRoot/kill-dev-ports.ps1" -Backend
    Write-Error @"
Gateway still not on ${GatewayPort}. Start backend (one instance):
  npm run dev:server
"@
    exit 1
}

$settingsPath = Join-Path $env:USERPROFILE ".cab\settings.json"
$settings = Get-Content $settingsPath | ConvertFrom-Json
$key = $settings.gateway_key
if ($settings.gateway_port -and [int]$settings.gateway_port -ne $GatewayPort) {
    Write-Error "settings.json gateway_port must be $GatewayPort"
    exit 1
}

$claude = Join-Path $env:USERPROFILE ".local\bin\claude.exe"
if (-not (Test-Path $claude)) {
    Write-Error "Claude Code CLI not found at $claude"
    exit 1
}

Write-Host "=== CC headless real test ==="
Write-Host "Gateway: http://localhost:${GatewayPort} | Model: $Model"

$null = Invoke-RestMethod -Uri "$Base/api/dashboard/stats" -Headers @{ Authorization = "Bearer $key" } -TimeoutSec 30
Write-Host "Gateway health: OK"

try {
    Invoke-RestMethod -Method Put -Uri "$Base/api/agents/claude-code" `
        -Headers @{ Authorization = "Bearer $key"; "Content-Type" = "application/json" } `
        -Body '{"mode":"auto","model_id":"auto"}' | Out-Null
} catch {
    Write-Warning "Agent config update skipped: $($_.Exception.Message)"
}

$explain = Invoke-RestMethod -Method Post -Uri "$Base/api/routing/explain" `
    -Headers @{ Authorization = "Bearer $key"; "Content-Type" = "application/json" } `
    -Body '{"agent":"claude-code","model":"auto"}'
if (-not $explain.resolved) {
    Write-Error @"
Auto routing unresolved — no enabled provider with API keys in ~/.cab/settings.json.
Open http://127.0.0.1:5173/providers and configure keys, then retry.
Detail: $($explain.decision_steps[-1].detail)
"@
    exit 1
}
Write-Host "Routing: $($explain.resolved.model_id) @ $($explain.resolved.provider_id)"

$before = (Invoke-RestMethod -Uri "$Base/api/logs?per_page=1&page=1" -Headers @{ Authorization = "Bearer $key" }).data[0].id

$env:ANTHROPIC_BASE_URL = "http://localhost:3125"
$env:ANTHROPIC_AUTH_TOKEN = $key
$env:CLAUDE_CODE_ENABLE_GATEWAY_MODEL_DISCOVERY = "1"

$psi = New-Object System.Diagnostics.ProcessStartInfo
$psi.FileName = $claude
$psi.Arguments = "-p `"$Prompt`" --model `"$Model`" --max-turns 3"
$psi.RedirectStandardOutput = $true
$psi.RedirectStandardError = $true
$psi.UseShellExecute = $false
try { $psi.StandardInputEncoding = [Text.Encoding]::UTF8 } catch {}
$psi.RedirectStandardInput = $true

$p = [System.Diagnostics.Process]::Start($psi)
$p.StandardInput.Close()
$stdout = $p.StandardOutput.ReadToEnd()
$stderr = $p.StandardError.ReadToEnd()
if (-not $p.WaitForExit($TimeoutSec * 1000)) {
    $p.Kill()
    Write-Error "Timed out after ${TimeoutSec}s"
    exit 1
}

Write-Host "--- Claude stdout ---"
Write-Host $stdout
if ($stderr) {
    Write-Host "--- Claude stderr ---"
    Write-Host $stderr
}
Write-Host "--- exit: $($p.ExitCode) ---"

Start-Sleep -Seconds 2
$after = (Invoke-RestMethod -Uri "$Base/api/logs?per_page=1&page=1" -Headers @{ Authorization = "Bearer $key" }).data[0]

if ($p.ExitCode -ne 0) { exit $p.ExitCode }
if ($after.id -ne $before) {
    Write-Host "OK: log id=$($after.id) provider=$($after.provider) model=$($after.model)"
    exit 0
}
if ($stdout -match "CAB ok") {
    Write-Host "OK: output matched"
    exit 0
}

Write-Error "FAIL: no new gateway log and unexpected output"
exit 1
