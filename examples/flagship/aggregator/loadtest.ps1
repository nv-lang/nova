# SPDX-License-Identifier: MIT OR Apache-2.0
# loadtest.ps1 -- self-contained load test for the flagship aggregator (Plan 187).
#
# Builds (optionally) and launches the server itself, hammers every endpoint/mode
# (sequential, concurrent, SSE, idle), prints PASS/FAIL per block, then kills the
# server. Depends on no background infrastructure -- runs on a clean machine.
# ASCII-only on purpose (robust under any PowerShell console codepage).
#
# Run (PowerShell in d:\Sources\nv-lang\nova):
#   .\examples\flagship\aggregator\loadtest.ps1
#   .\examples\flagship\aggregator\loadtest.ps1 -Port 8195 -Build   # rebuild binary
#   .\examples\flagship\aggregator\loadtest.ps1 -SkipLive           # no real internet
#   .\examples\flagship\aggregator\loadtest.ps1 -Iterations 20      # heavier
#
# Needs a built aggregator_demo.exe (or -Build) and the GC lib in vcpkg_installed.

param(
    [int]$Port = 8195,
    [int]$Iterations = 5,
    [int]$Concurrency = 8,
    [switch]$Build,
    [switch]$SkipLive,
    [string]$RepoRoot = "d:\Sources\nv-lang\nova"
)

$ErrorActionPreference = "Stop"
$bin   = Join-Path $RepoRoot "aggregator_demo.exe"
$gcLib = Join-Path $RepoRoot "compiler-codegen\vcpkg_installed\x64-windows-static\lib"
$gcInc = Join-Path $RepoRoot "compiler-codegen\vcpkg_installed\x64-windows-static\include"
$B = "http://127.0.0.1:$Port"

$script:pass = 0; $script:fail = 0; $script:failed = @()
function Check([bool]$ok, [string]$name) {
    if ($ok) { $script:pass++ } else { $script:fail++; $script:failed += $name; Write-Host "    x FAIL: $name" -ForegroundColor Red }
}
function Code([string]$url, [int]$timeoutSec = 20) {
    try { return (Invoke-WebRequest -Uri $url -TimeoutSec $timeoutSec -UseBasicParsing).StatusCode } catch { return 0 }
}
function Alive() { return (Code "$B/" 6) -eq 200 }
function AliveStr() { if (Alive) { return "200" } else { return "000" } }

# -- build (optional) --------------------------------------------------------
if ($Build -or -not (Test-Path $bin)) {
    Write-Host "Building aggregator_demo.exe ..." -ForegroundColor Cyan
    $env:NOVA_GC_LIB_DIR = $gcLib; $env:NOVA_INCLUDE_DIR = $gcInc; $env:NOVA_GC_INCLUDE_DIR = $gcInc
    $localToml = Join-Path $RepoRoot "examples\nova.local.toml"
    if (-not (Test-Path $localToml)) {
        "[replace]`ntls = { path = `"../../nova-tls`" }`nhttp = { path = `"../../nova-http`" }" | Out-File -Encoding utf8 $localToml
    }
    Push-Location $RepoRoot
    & (Join-Path $RepoRoot "nova-cli\target\release\nova.exe") build "examples\flagship\aggregator\src\main.nv" -o $bin
    Pop-Location
    if (-not (Test-Path $bin)) { Write-Host "Build failed" -ForegroundColor Red; exit 1 }
}

# -- free port + launch server -----------------------------------------------
Get-NetTCPConnection -LocalPort $Port -ErrorAction SilentlyContinue |
    Select-Object -Expand OwningProcess -Unique | Where-Object { $_ -ne 0 } |
    ForEach-Object { Stop-Process -Id $_ -Force -ErrorAction SilentlyContinue }
Start-Sleep 1

$env:NOVA_GC_LIB_DIR = $gcLib; $env:AGGREGATOR_PORT = "$Port"
Write-Host "Launching server on $B ..." -ForegroundColor Cyan
$srv = Start-Process -FilePath $bin -WorkingDirectory $RepoRoot -PassThru -WindowStyle Hidden
$ready = $false
for ($i = 0; $i -lt 30; $i++) { Start-Sleep -Milliseconds 500; if (Alive) { $ready = $true; break } }
if (-not $ready) { Write-Host "Server did not come up" -ForegroundColor Red; if (-not $srv.HasExited) { Stop-Process -Id $srv.Id -Force }; exit 1 }
Write-Host "Server ready (PID $($srv.Id))`n" -ForegroundColor Green

$legends = @("weather", "health")
$modes = if ($SkipLive) { @("demo", "chaos") } else { @("demo", "chaos", "live") }

try {
    # -- BLOCK 1: base endpoints ---------------------------------------------
    Write-Host "== BLOCK 1: base endpoints ==" -ForegroundColor Yellow
    foreach ($e in @("/", "/api/snapshot")) { $c = Code "$B$e"; Check ($c -eq 200) "GET $e"; Write-Host "    $e -> $c" }

    # -- BLOCK 2: /api/run every legend x mode -------------------------------
    Write-Host "== BLOCK 2: /api/run (legend x mode) ==" -ForegroundColor Yellow
    foreach ($l in $legends) { foreach ($m in $modes) {
        $c = Code "$B/api/run?legend=$l&mode=$m&seed=42" 25; Check ($c -eq 200) "run $l/$m"; Write-Host "    $l/$m -> $c" } }
    Check (Alive) "alive after BLOCK 2"

    # -- BLOCK 3: /api/events (SSE) every legend x mode ----------------------
    Write-Host "== BLOCK 3: /api/events SSE (legend x mode) ==" -ForegroundColor Yellow
    foreach ($l in $legends) { foreach ($m in $modes) {
        try {
            $body = (Invoke-WebRequest -Uri "$B/api/events?legend=$l&mode=$m&seed=42" -TimeoutSec 20 -UseBasicParsing).Content
            $n = ([regex]::Matches($body, "(?m)^event:")).Count
            Check ($n -ge 2 -and (Alive)) "sse $l/$m ($n events)"; Write-Host "    $l/$m -> events=$n, server=$(AliveStr)"
        } catch { Check $false "sse $l/$m (exception)" }
    } }

    # -- BLOCK 4: sustained SSE weather-live xN (historic wedge case) --------
    if (-not $SkipLive) {
        Write-Host "== BLOCK 4: sustained SSE weather-live x$Iterations (used to hang on #2) ==" -ForegroundColor Yellow
        for ($i = 1; $i -le $Iterations; $i++) {
            try { $body = (Invoke-WebRequest -Uri "$B/api/events?legend=weather&mode=live" -TimeoutSec 20 -UseBasicParsing).Content
                  $n = ([regex]::Matches($body, "(?m)^event:")).Count } catch { $n = 0 }
            Check ($n -ge 2 -and (Alive)) "sustained-sse $i"; Write-Host "    $i -> events=$n, server=$(AliveStr)"
        }
    }

    # -- BLOCK 5: concurrency -- N parallel /api/run -------------------------
    Write-Host "== BLOCK 5: concurrency $Concurrency parallel /api/run ==" -ForegroundColor Yellow
    $jobs = 1..$Concurrency | ForEach-Object {
        $m = @("demo", "chaos")[($_ % 2)]
        Start-Job -ScriptBlock { param($u) try { (Invoke-WebRequest -Uri $u -TimeoutSec 25 -UseBasicParsing).StatusCode } catch { 0 } } -ArgumentList "$B/api/run?legend=health&mode=$m&seed=$_"
    }
    $codes = $jobs | Wait-Job -Timeout 40 | Receive-Job
    $jobs | Remove-Job -Force -ErrorAction SilentlyContinue
    $ok = ($codes | Where-Object { $_ -eq 200 }).Count
    Check ($ok -eq $Concurrency -and (Alive)) "concurrency $ok/$Concurrency"; Write-Host "    200 responses: $ok/$Concurrency, server=$(AliveStr)"

    # -- BLOCK 6: idle survival (watchdog-idle regression) -------------------
    Write-Host "== BLOCK 6: 12s idle (watchdog-idle) ==" -ForegroundColor Yellow
    Start-Sleep 12
    Check (Alive) "alive after 12s idle"; Write-Host "    after idle: $(AliveStr)"

    # -- BLOCK 7: demo determinism (same seed => same per-source outcomes) ----
    # Compare the SET of id:state (sorted), NOT array order: `parallel for`
    # settles lanes in nondeterministic completion order, so the results[]
    # order varies run-to-run while each source's outcome is fixed (demo table).
    Write-Host "== BLOCK 7: demo determinism seed=42 (per-source outcomes) ==" -ForegroundColor Yellow
    function Outcomes($url) { try { (((Invoke-WebRequest -Uri $url -TimeoutSec 20 -UseBasicParsing).Content | ConvertFrom-Json).results |
        ForEach-Object { "$($_.id):$($_.status.state)" } | Sort-Object) } catch { @() } }
    $a = Outcomes "$B/api/run?legend=weather&mode=demo&seed=42"
    $b = Outcomes "$B/api/run?legend=weather&mode=demo&seed=42"
    Check (($a -join ",") -eq ($b -join ",") -and $a.Count -gt 0) "demo determinism (per-source outcomes match)"
    Write-Host "    outcomes: $($a -join ', ')"
}
finally {
    if (-not $srv.HasExited) { Stop-Process -Id $srv.Id -Force -ErrorAction SilentlyContinue }
    Get-NetTCPConnection -LocalPort $Port -ErrorAction SilentlyContinue |
        Select-Object -Expand OwningProcess -Unique | Where-Object { $_ -ne 0 } |
        ForEach-Object { Stop-Process -Id $_ -Force -ErrorAction SilentlyContinue }
}

# -- SUMMARY -----------------------------------------------------------------
Write-Host "`n====================================" -ForegroundColor Cyan
$color = if ($script:fail -eq 0) { "Green" } else { "Red" }
Write-Host "RESULT: PASS=$($script:pass)  FAIL=$($script:fail)" -ForegroundColor $color
if ($script:fail -gt 0) { Write-Host "Failed: $($script:failed -join '; ')" -ForegroundColor Red; exit 1 }
Write-Host "All load blocks green." -ForegroundColor Green
