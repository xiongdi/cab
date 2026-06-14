# Kill processes listening on CAB dev ports (5173 / 3125). See AGENTS.md.
param(
    [switch]$Frontend,
    [switch]$Backend,
    [switch]$All
)

if (-not $Frontend -and -not $Backend) { $All = $true }
$ports = @()
if ($All -or $Frontend) { $ports += 5173 }
if ($All -or $Backend) { $ports += 3125 }

foreach ($port in $ports) {
    $lines = netstat -ano | Select-String ":$port\s+.*LISTENING"
    if (-not $lines) {
        Write-Host "Port ${port}: free"
        continue
    }
    $pids = $lines | ForEach-Object {
        ($_ -split '\s+')[-1]
    } | Sort-Object -Unique
    foreach ($procId in $pids) {
        if ($procId -eq '0') { continue }
        Write-Host "Killing PID $procId on port $port"
        Stop-Process -Id ([int]$procId) -Force -ErrorAction SilentlyContinue
    }
    Start-Sleep -Milliseconds 500
    if (netstat -ano | Select-String ":$port\s+.*LISTENING") {
        Write-Error "Port $port still in use after kill"
        exit 1
    }
    Write-Host "Port ${port}: released"
}
