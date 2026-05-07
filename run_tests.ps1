param([string]$Filter = "", [switch]$IncludeStdlib)

$ErrorActionPreference = "Continue"
$codegen = "d:\Sources\nova-lang\compiler-codegen\target\debug\nova-codegen.exe"
$rt_dir = "d:\Sources\nova-lang\compiler-codegen\nova_rt"
$tests_dir = "d:\Sources\nova-lang\tests-nova"
$stdlib_dir = "d:\Sources\nova-lang\examples\stdlib"
$tmp_dir = "C:\Temp\nova_tests"
$vcvars = "C:\Program Files (x86)\Microsoft Visual Studio\18\BuildTools\VC\Auxiliary\Build\vcvars64.bat"
New-Item -ItemType Directory -Force -Path $tmp_dir | Out-Null

$pass = 0; $fail = 0
$results = @()

# Collect input files: tests-nova (recursive: подкаталоги groups/), stdlib if -IncludeStdlib.
$inputs = @(Get-ChildItem -Path $tests_dir -Filter "*.nv" -Recurse -File)
if ($IncludeStdlib) {
    $inputs += @(Get-ChildItem -Path $stdlib_dir -Filter "*.nv" -Recurse -File)
}
# Sort by relative path so group/file order is predictable, group-by-group.
$inputs | Sort-Object FullName | ForEach-Object {
    $nv = $_.FullName
    $name = $_.BaseName
    # Display name includes parent group для multi-level layout. Используем
    # подходящую базу: tests_dir для tests-nova/* или stdlib_dir для examples/stdlib/*.
    # Case-insensitive — Windows может вернуть FullName с любым регистром drive letter.
    $full_lower = $_.FullName.ToLower()
    $is_stdlib  = $full_lower.StartsWith($stdlib_dir.ToLower())
    $base = if ($full_lower.StartsWith($tests_dir.ToLower())) { $tests_dir }
            elseif ($is_stdlib) { $stdlib_dir }
            else { Split-Path $_.FullName -Parent }
    $relative = $_.FullName.Substring($base.Length).TrimStart('\').TrimStart('/')
    $relative = $relative -replace '\\', '/'
    if ($is_stdlib) {
        # Префикс "stdlib/" чтобы display было нагрузочно-говорящим
        $display = "stdlib/" + ($relative -replace '\.nv$', '')
    } else {
        $display = $relative -replace '\.nv$', ''
    }
    if ($Filter -and $display -notlike "*$Filter*") { return }

    # .c file is emitted next to .nv by the codegen, regardless of source dir.
    $c_file = Join-Path $_.Directory.FullName "$name.c"
    # Уникальный exe name через display (group__file), чтобы избежать
    # коллизий при одинаковых basename в разных группах.
    $exe_safe = ($display -replace '[/\\]', '__')
    $exe_file = "$tmp_dir\$exe_safe.exe"

    # Step 1: codegen .nv -> .c
    $cg_out = & $codegen compile $nv 2>&1
    if ($LASTEXITCODE -ne 0) {
        $results += [PSCustomObject]@{Name=$display; Status="CODEGEN-FAIL"; Detail=($cg_out -join " " | ForEach-Object { if ($_.Length -gt 100) { $_.Substring(0,100) } else { $_ } })}
        $fail++; return
    }
    if (-not (Test-Path $c_file)) {
        $results += [PSCustomObject]@{Name=$display; Status="NO-C-FILE"; Detail=""}
        $fail++; return
    }

    # Step 2: compile .c -> .exe via MSVC.
    # Используем per-test obj-каталог, чтобы избежать коллизий имён
    # (test `effects/effects.c` иначе перезапишет runtime `effects.obj`).
    $obj_dir = "$tmp_dir\$exe_safe-obj"
    New-Item -ItemType Directory -Force -Path $obj_dir | Out-Null
    $cl_cmd = "cl.exe /nologo /W0 /I `"d:\Sources\nova-lang\compiler-codegen`" /Fo`"$obj_dir\\`" /Fe`"$exe_file`" `"$c_file`" `"$rt_dir\alloc.c`" `"$rt_dir\effects.c`" `"$rt_dir\fibers.c`""
    $cl_out = cmd /c "`"$vcvars`" && $cl_cmd" 2>&1
    if ($LASTEXITCODE -ne 0) {
        $errs = ($cl_out | Where-Object { $_ -match "error" } | Select-Object -First 3) -join " | "
        $results += [PSCustomObject]@{Name=$display; Status="CC-FAIL"; Detail=$errs}
        $fail++; return
    }

    # Step 3: run
    $run_out = & $exe_file 2>&1
    if ($LASTEXITCODE -ne 0) {
        $results += [PSCustomObject]@{Name=$display; Status="RUN-FAIL"; Detail=(($run_out | Select-Object -Last 3) -join " | ")}
        $fail++; return
    }

    $results += [PSCustomObject]@{Name=$display; Status="PASS"; Detail=""}
    $pass++
}

Write-Host ""
Write-Host "===== RESULTS =====" -ForegroundColor Cyan
foreach ($r in $results) {
    $color = if ($r.Status -eq "PASS") { "Green" } elseif ($r.Status -match "FAIL|NO-C") { "Red" } else { "Yellow" }
    $line = "$($r.Status.PadRight(14)) $($r.Name)"
    if ($r.Detail) { $line += "  # $($r.Detail.Substring(0, [Math]::Min(120, $r.Detail.Length)))" }
    Write-Host $line -ForegroundColor $color
}
Write-Host ""
Write-Host "PASS: $pass  FAIL: $fail" -ForegroundColor Cyan
