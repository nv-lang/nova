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
    [int]$Iterations = 50,          # sustained SSE weather-live repeats (10x baseline)
    [int]$Concurrency = 80,         # parallel /api/run via runspace pool (10x baseline)
    [int]$Rounds = 10,              # times to repeat the run/SSE combo sweeps (demo/chaos)
    [int]$LiveRounds = 0,           # live-combo rounds cap; 0 = min(Rounds, 10).
                                    # Live rounds hit REAL external hosts (open-meteo has a
                                    # 10k req/day free limit) — scale demo/chaos freely, keep
                                    # live polite. BLOCK 4 (Iterations) is intentionally NOT
                                    # capped: it IS the sustained-live marathon (mind the quota).
    [switch]$Build,
    [switch]$SkipLive,
    [string]$RepoRoot = "d:\Sources\nv-lang\nova"
)
if ($LiveRounds -le 0) { $LiveRounds = [Math]::Min($Rounds, 10) }

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
    # cmd /c + 2>&1: PowerShell 5.1 wraps native stderr lines (even warnings)
    # into NativeCommandError records, which $ErrorActionPreference="Stop"
    # escalates into a script abort — route through cmd to keep them as text.
    cmd /c "`"$(Join-Path $RepoRoot 'nova-cli\target\release\nova.exe')`" build examples\flagship\aggregator\src\main.nv -o `"$bin`" 2>&1" | Out-Host
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
# Server output goes to log files — the ONLY post-mortem evidence when a
# marathon kills the server mid-run ([M-187-sustained-live-tls-resource-death]:
# died forever at sustained-SSE #274 with no log to diagnose).
$srvOut = Join-Path $env:TEMP "aggregator-loadtest-$Port.out.log"
$srvErr = Join-Path $env:TEMP "aggregator-loadtest-$Port.err.log"
Remove-Item $srvOut, $srvErr -Force -ErrorAction SilentlyContinue
$srv = Start-Process -FilePath $bin -WorkingDirectory $RepoRoot -PassThru -WindowStyle Hidden `
    -RedirectStandardOutput $srvOut -RedirectStandardError $srvErr
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

    # -- BLOCK 2: /api/run every legend x mode (live combos capped at $LiveRounds) --
    Write-Host "== BLOCK 2: /api/run (legend x mode) x$Rounds rounds (live x$LiveRounds) ==" -ForegroundColor Yellow
    foreach ($l in $legends) { foreach ($m in $modes) {
        $n2 = if ($m -eq "live") { $LiveRounds } else { $Rounds }
        $ok2 = 0
        for ($r = 0; $r -lt $n2; $r++) { if ((Code "$B/api/run?legend=$l&mode=$m&seed=42" 25) -eq 200) { $ok2++ } }
        Check ($ok2 -eq $n2) "run $l/$m ($ok2/$n2)"; Write-Host "    $l/$m -> $ok2/$n2" } }
    Check (Alive) "alive after BLOCK 2"

    # -- BLOCK 3: /api/events (SSE) every legend x mode (live capped) --------
    Write-Host "== BLOCK 3: /api/events SSE (legend x mode) x$Rounds rounds (live x$LiveRounds) ==" -ForegroundColor Yellow
    foreach ($l in $legends) { foreach ($m in $modes) {
        $n3 = if ($m -eq "live") { $LiveRounds } else { $Rounds }
        $ok3 = 0; $lastN = 0
        for ($r = 0; $r -lt $n3; $r++) {
            try {
                $body = (Invoke-WebRequest -Uri "$B/api/events?legend=$l&mode=$m&seed=42" -TimeoutSec 20 -UseBasicParsing).Content
                $lastN = ([regex]::Matches($body, "(?m)^event:")).Count
                if ($lastN -ge 2) { $ok3++ }
            } catch { }
        }
        Check ($ok3 -eq $n3 -and (Alive)) "sse $l/$m ($ok3/$n3)"; Write-Host "    $l/$m -> $ok3/$n3 ok (events~$lastN), server=$(AliveStr)"
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

    # -- BLOCK 5: concurrency -- N truly-parallel /api/run (runspace pool) ----
    # Runspaces = lightweight threads INSIDE one process (not Start-Job, which
    # forks a full powershell.exe per job -> 80 of those would exhaust the box).
    # This drives N SIMULTANEOUS HTTP connections at the server cheaply.
    Write-Host "== BLOCK 5: concurrency $Concurrency parallel /api/run (runspace pool) ==" -ForegroundColor Yellow
    $pool = [runspacefactory]::CreateRunspacePool(1, $Concurrency); $pool.Open()
    $work = @()
    for ($k = 1; $k -le $Concurrency; $k++) {
        $m = @("demo", "chaos")[($k % 2)]
        $ps = [powershell]::Create(); $ps.RunspacePool = $pool
        [void]$ps.AddScript({ param($u) try { (Invoke-WebRequest -Uri $u -TimeoutSec 30 -UseBasicParsing).StatusCode } catch { 0 } }).AddArgument("$B/api/run?legend=health&mode=$m&seed=$k")
        $work += [pscustomobject]@{ ps = $ps; handle = $ps.BeginInvoke() }
    }
    $codes = $work | ForEach-Object { try { $_.ps.EndInvoke($_.handle) } catch { 0 } finally { $_.ps.Dispose() } }
    $pool.Close(); $pool.Dispose()
    $ok = ($codes | Where-Object { $_ -eq 200 }).Count
    # Criterion = SURVIVAL, not throughput: bounded-accept (admission control,
    # [M-187-high-concurrency-connection-wedge] mitigation) intentionally sheds
    # load above MAX inflight with an honest close. PASS = at least one request
    # served AND the server is alive AND a follow-up single request succeeds
    # (i.e. no wedge). Shed count is informational.
    $post = Code "$B/api/run?legend=weather&mode=demo&seed=42" 20
    Check ($ok -ge 1 -and (Alive) -and $post -eq 200) "concurrency survival (served=$ok/$Concurrency, post=$post)"
    Write-Host "    served: $ok/$Concurrency (rest honestly shed), post-single: $post, server=$(AliveStr)"

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
if ($script:fail -gt 0) {
    Write-Host "-- server log tails (post-mortem) --" -ForegroundColor Yellow
    foreach ($f in @($srvOut, $srvErr)) {
        if ((Test-Path $f) -and (Get-Item $f).Length -gt 0) {
            Write-Host "[$f]:"; Get-Content $f -Tail 15 | ForEach-Object { Write-Host "    $_" }
        } else { Write-Host "[$f]: empty" }
    }
}
if ($script:fail -gt 0) { Write-Host "Failed: $($script:failed -join '; ')" -ForegroundColor Red; exit 1 }
Write-Host "All load blocks green." -ForegroundColor Green
