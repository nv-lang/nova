# SPDX-License-Identifier: MIT OR Apache-2.0
# tls-leak-repro.ps1 -- DETERMINISTIC repro/gate for
# [M-187-sustained-live-tls-resource-death] (nova-tls live-TLS heap leak).
#
# Launches the aggregator, fires N weather-live requests (real open-meteo HTTPS —
# the ONLY path that leaks; demo/chaos/health-TCP do not), samples the server's
# private commit every -Sample runs, fits the least-squares slope in MB/run, and
# prints a BINARY verdict:
#     LEAK  if slope >= -SlopeFailMB  (baseline before fix: ~1.4 MB/run)
#     CLEAN if slope <  -SlopeFailMB  (target after fix: flat, ~0)
# No need to run to server death (~670 runs) -- the slope is the deterministic signal.
# ASCII-only; numbers formatted InvariantCulture (avoid RU comma-decimal).
#
# Run (PowerShell in d:\Sources\nv-lang\nova):
#   .\examples\flagship\aggregator\tls-leak-repro.ps1              # 80 runs (polite to open-meteo)
#   .\examples\flagship\aggregator\tls-leak-repro.ps1 -Runs 120 -Build
#
# open-meteo free limit = 10k/day: default 80 runs is cheap; don't loop this on the API needlessly.

param(
    [int]$Port = 8212,
    [int]$Runs = 80,
    [int]$Sample = 10,               # sample memory every N runs
    [double]$SlopeFailMB = 0.30,     # MB/run; >= this = LEAK (baseline 1.4, clean ~0)
    [switch]$Build,
    [string]$RepoRoot = "d:\Sources\nv-lang\nova"
)
$ErrorActionPreference = "Stop"
$ci  = [System.Globalization.CultureInfo]::InvariantCulture
$bin = Join-Path $RepoRoot "aggregator_demo.exe"
$B   = "http://127.0.0.1:$Port"
function Alive { try { (Invoke-WebRequest "$B/" -TimeoutSec 6 -UseBasicParsing).StatusCode -eq 200 } catch { $false } }

if ($Build -or -not (Test-Path $bin)) {
    Write-Host "Building aggregator_demo.exe ..." -ForegroundColor Cyan
    $env:NOVA_GC_LIB_DIR = Join-Path $RepoRoot "compiler-codegen\vcpkg_installed\x64-windows-static\lib"
    $env:NOVA_INCLUDE_DIR = Join-Path $RepoRoot "compiler-codegen\vcpkg_installed\x64-windows-static\include"
    $env:NOVA_GC_INCLUDE_DIR = $env:NOVA_INCLUDE_DIR
    Push-Location $RepoRoot
    cmd /c "`"$(Join-Path $RepoRoot 'nova-cli\target\release\nova.exe')`" build examples\flagship\aggregator\src\main.nv -o `"$bin`" 2>&1" | Out-Host
    Pop-Location
    if (-not (Test-Path $bin)) { Write-Host "build failed" -ForegroundColor Red; exit 2 }
}

# free port + launch
Get-NetTCPConnection -LocalPort $Port -ErrorAction SilentlyContinue |
    Select-Object -Expand OwningProcess -Unique | Where-Object { $_ -ne 0 } |
    ForEach-Object { Stop-Process -Id $_ -Force -ErrorAction SilentlyContinue }
Start-Sleep 1
$env:NOVA_GC_LIB_DIR = Join-Path $RepoRoot "compiler-codegen\vcpkg_installed\x64-windows-static\lib"
$env:AGGREGATOR_PORT = "$Port"
$srv = Start-Process -FilePath $bin -WorkingDirectory $RepoRoot -PassThru -WindowStyle Hidden
$ready = $false
for ($i = 0; $i -lt 30; $i++) { Start-Sleep -Milliseconds 500; if (Alive) { $ready = $true; break } }
if (-not $ready) { Write-Host "server did not start" -ForegroundColor Red; if (-not $srv.HasExited) { Stop-Process -Id $srv.Id -Force }; exit 2 }
$procId = (Get-NetTCPConnection -LocalPort $Port -State Listen | Select-Object -First 1 -Expand OwningProcess)
Write-Host "server ready (PID $procId); firing $Runs weather-live runs, sampling every $Sample`n" -ForegroundColor Green

function PrivMB { (Get-Process -Id $procId -ErrorAction SilentlyContinue).PrivateMemorySize64 / 1MB }

$xs = New-Object System.Collections.Generic.List[double]
$ys = New-Object System.Collections.Generic.List[double]
$fails = 0
try {
    # warmup (first-touch allocations are not part of the per-run slope)
    1..3 | ForEach-Object { try { Invoke-WebRequest "$B/api/run?legend=weather&mode=live" -TimeoutSec 20 -UseBasicParsing | Out-Null } catch {} }
    $base = PrivMB
    Write-Host ("run    0: private = {0} MB  (baseline)" -f [math]::Round($base,1))
    for ($r = 1; $r -le $Runs; $r++) {
        try { Invoke-WebRequest "$B/api/run?legend=weather&mode=live" -TimeoutSec 25 -UseBasicParsing | Out-Null }
        catch { $fails++ }
        if (($r % $Sample) -eq 0) {
            if (-not (Get-Process -Id $procId -ErrorAction SilentlyContinue)) { Write-Host "SERVER DIED at run $r (OOM?)" -ForegroundColor Red; break }
            $m = PrivMB; $xs.Add([double]$r); $ys.Add([double]$m)
            Write-Host ("run {0,4}: private = {1} MB" -f $r, [math]::Round($m,1))
        }
    }
} finally {
    if ($srv -and -not $srv.HasExited) { Stop-Process -Id $srv.Id -Force -ErrorAction SilentlyContinue }
    Get-NetTCPConnection -LocalPort $Port -ErrorAction SilentlyContinue |
        Select-Object -Expand OwningProcess -Unique | Where-Object { $_ -ne 0 } |
        ForEach-Object { Stop-Process -Id $_ -Force -ErrorAction SilentlyContinue }
}

if ($xs.Count -lt 2) { Write-Host "not enough samples (server died early = leak-consistent)" -ForegroundColor Red; exit 1 }
# least-squares slope (MB per run)
$n = $xs.Count; $sx = 0.0; $sy = 0.0; $sxy = 0.0; $sxx = 0.0
for ($i = 0; $i -lt $n; $i++) { $sx += $xs[$i]; $sy += $ys[$i]; $sxy += $xs[$i]*$ys[$i]; $sxx += $xs[$i]*$xs[$i] }
$slope = ($n*$sxy - $sx*$sy) / ($n*$sxx - $sx*$sx)
$grew  = [math]::Round($ys[$n-1] - $ys[0], 1)

Write-Host "`n====================================" -ForegroundColor Cyan
Write-Host ("slope = {0} MB/run  (grew {1} MB over {2} sampled runs; failures {3})" -f `
    [math]::Round($slope,3).ToString($ci), $grew, ($xs[$n-1]-$xs[0]), $fails)
if ($slope -ge $SlopeFailMB) {
    Write-Host ("VERDICT: LEAK  (slope >= {0} MB/run threshold) -- [M-187-sustained-live-tls-resource-death] REPRODUCED" -f $SlopeFailMB.ToString($ci)) -ForegroundColor Red
    exit 1
} else {
    Write-Host ("VERDICT: CLEAN (slope < {0} MB/run threshold) -- no runaway growth" -f $SlopeFailMB.ToString($ci)) -ForegroundColor Green
    exit 0
}
