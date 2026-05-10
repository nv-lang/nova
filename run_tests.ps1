param([string]$Filter = "", [switch]$IncludeStdlib)

$ErrorActionPreference = "Continue"

# Все пути относительно расположения этого скрипта — он лежит в корне
# репозитория. Это позволяет clon-нуть репо в любой каталог и запускать
# без правки путей. Override через env-vars при необходимости.
$repo_root  = $PSScriptRoot
$codegen    = if ($env:NOVA_CODEGEN)    { $env:NOVA_CODEGEN }    else { Join-Path $repo_root "compiler-codegen\target\debug\nova-codegen.exe" }
$rt_dir     = if ($env:NOVA_RT_DIR)     { $env:NOVA_RT_DIR }     else { Join-Path $repo_root "compiler-codegen\nova_rt" }
$tests_dir  = if ($env:NOVA_TESTS_DIR)  { $env:NOVA_TESTS_DIR }  else { Join-Path $repo_root "nova_tests" }
$stdlib_dir = if ($env:NOVA_STDLIB_DIR) { $env:NOVA_STDLIB_DIR } else { Join-Path $repo_root "std" }
$tmp_dir    = if ($env:NOVA_TMP_DIR)    { $env:NOVA_TMP_DIR }    else { Join-Path $env:TEMP "nova_tests" }
$cg_inc_dir = if ($env:NOVA_INCLUDE)    { $env:NOVA_INCLUDE }    else { Join-Path $repo_root "compiler-codegen" }

# vcvars: override через NOVA_VCVARS, иначе пытаемся найти через vswhere.
# vswhere — стандартная утилита, поставляется с любой VS Installer'ом.
if ($env:NOVA_VCVARS) {
    $vcvars = $env:NOVA_VCVARS
} else {
    $vswhere = Join-Path ${env:ProgramFiles(x86)} "Microsoft Visual Studio\Installer\vswhere.exe"
    if (Test-Path $vswhere) {
        $vcvars = & $vswhere -latest -products * -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -find "VC\Auxiliary\Build\vcvars64.bat" | Select-Object -First 1
    }
    if (-not $vcvars -or -not (Test-Path $vcvars)) {
        Write-Host "ERROR: vcvars64.bat not found. Set NOVA_VCVARS env-var or install Visual Studio Build Tools." -ForegroundColor Red
        exit 1
    }
}

New-Item -ItemType Directory -Force -Path $tmp_dir | Out-Null

$pass = 0; $fail = 0
$results = @()

# Collect input files: nova_tests (recursive: подкаталоги groups/), stdlib if -IncludeStdlib.
$inputs = @(Get-ChildItem -Path $tests_dir -Filter "*.nv" -Recurse -File)
if ($IncludeStdlib) {
    $inputs += @(Get-ChildItem -Path $stdlib_dir -Filter "*.nv" -Recurse -File)
}
# Sort by relative path so group/file order is predictable, group-by-group.
$inputs | Sort-Object FullName | ForEach-Object {
    $nv = $_.FullName
    $name = $_.BaseName
    # Display name includes parent group для multi-level layout. Используем
    # подходящую базу: tests_dir для nova_tests/* или stdlib_dir для std/*.
    # Case-insensitive — Windows может вернуть FullName с любым регистром drive letter.
    $full_lower = $_.FullName.ToLower()
    $is_stdlib  = $full_lower.StartsWith($stdlib_dir.ToLower())
    $base = if ($full_lower.StartsWith($tests_dir.ToLower())) { $tests_dir }
            elseif ($is_stdlib) { $stdlib_dir }
            else { Split-Path $_.FullName -Parent }
    $relative = $_.FullName.Substring($base.Length).TrimStart('\').TrimStart('/')
    $relative = $relative -replace '\\', '/'
    if ($is_stdlib) {
        # Префикс "std/" чтобы display было нагрузочно-говорящим
        $display = "std/" + ($relative -replace '\.nv$', '')
    } else {
        $display = $relative -replace '\.nv$', ''
    }
    if ($Filter -and $display -notlike "*$Filter*") { return }

    # D89 (spec/decisions/09-tooling.md): EXPECT_* test-tooling маркеры.
    #
    # Поддерживаемые маркеры (substring-pattern или integer):
    #   // EXPECT_COMPILE_ERROR <pattern>  — codegen упал с pattern
    #   // EXPECT_RUNTIME_PANIC <pattern>  — exe упал с panic, stderr содержит pattern
    #   // EXPECT_EXIT_CODE <N>            — exe завершился с exit code = N
    #   // EXPECT_STDOUT <pattern>         — только stdout содержит pattern
    #   // EXPECT_STDERR <pattern>         — только stderr содержит pattern
    #
    # Один маркер на файл (берём первый найденный в первых 30 строках).
    # Маркеры взаимоисключающие.
    $expect_kind = $null      # 'compile_error' | 'runtime_panic' | 'exit_code' | 'stdout' | 'stderr'
    $expect_arg  = $null      # substring или int (для exit_code)
    $head = Get-Content -Path $nv -TotalCount 30 -ErrorAction SilentlyContinue
    foreach ($ln in $head) {
        if ($ln -match '//\s*EXPECT_COMPILE_ERROR\s+(.+?)\s*$') {
            $expect_kind = 'compile_error'; $expect_arg = $matches[1]; break
        }
        if ($ln -match '//\s*EXPECT_RUNTIME_PANIC\s+(.+?)\s*$') {
            $expect_kind = 'runtime_panic'; $expect_arg = $matches[1]; break
        }
        if ($ln -match '//\s*EXPECT_EXIT_CODE\s+(\d+)\s*$') {
            $expect_kind = 'exit_code'; $expect_arg = [int]$matches[1]; break
        }
        if ($ln -match '//\s*EXPECT_STDOUT\s+(.+?)\s*$') {
            $expect_kind = 'stdout'; $expect_arg = $matches[1]; break
        }
        if ($ln -match '//\s*EXPECT_STDERR\s+(.+?)\s*$') {
            $expect_kind = 'stderr'; $expect_arg = $matches[1]; break
        }
    }

    # .c file is emitted next to .nv by the codegen, regardless of source dir.
    $c_file = Join-Path $_.Directory.FullName "$name.c"
    # Уникальный exe name через display (group__file), чтобы избежать
    # коллизий при одинаковых basename в разных группах.
    $exe_safe = ($display -replace '[/\\]', '__')
    $exe_file = "$tmp_dir\$exe_safe.exe"

    # Step 1: codegen .nv -> .c
    $cg_out = & $codegen compile $nv 2>&1
    $cg_exit = $LASTEXITCODE

    # D89: EXPECT_COMPILE_ERROR — обрабатывается на этапе codegen.
    # Файл не компилируется .c → .exe и не запускается.
    if ($expect_kind -eq 'compile_error') {
        if ($cg_exit -eq 0) {
            $results += [PSCustomObject]@{
                Name=$display; Status="NEG-NO-ERROR";
                Detail="expected `// EXPECT_COMPILE_ERROR $expect_arg` but codegen succeeded"
            }
            $fail++; return
        }
        $cg_text = ($cg_out -join " ")
        if ($cg_text -notmatch [regex]::Escape($expect_arg)) {
            $snippet = if ($cg_text.Length -gt 150) { $cg_text.Substring(0,150) } else { $cg_text }
            $results += [PSCustomObject]@{
                Name=$display; Status="NEG-WRONG-MSG";
                Detail="expected pattern '$expect_arg' not found in: $snippet"
            }
            $fail++; return
        }
        $results += [PSCustomObject]@{Name=$display; Status="PASS"; Detail="(negative)"}
        $pass++; return
    }

    if ($cg_exit -ne 0) {
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
    $cl_cmd = "cl.exe /nologo /W0 /I `"$cg_inc_dir`" /Fo`"$obj_dir\\`" /Fe`"$exe_file`" `"$c_file`" `"$rt_dir\alloc.c`" `"$rt_dir\effects.c`" `"$rt_dir\fibers.c`""
    $cl_out = cmd /c "`"$vcvars`" && $cl_cmd" 2>&1
    if ($LASTEXITCODE -ne 0) {
        $errs = ($cl_out | Where-Object { $_ -match "error" } | Select-Object -First 3) -join " | "
        $results += [PSCustomObject]@{Name=$display; Status="CC-FAIL"; Detail=$errs}
        $fail++; return
    }

    # Step 3: run
    # D89: stdout и stderr — разные потоки, EXPECT_STDOUT и EXPECT_STDERR
    # проверяют их независимо. Используем временный stderr-файл и
    # читаем оба после выполнения.
    $stderr_file = "$tmp_dir\$exe_safe.stderr.txt"
    $stdout_out = & $exe_file 2>$stderr_file
    $run_exit = $LASTEXITCODE
    $stdout_text = if ($stdout_out) { ($stdout_out -join " ") } else { "" }
    $stderr_text = if (Test-Path $stderr_file) { (Get-Content $stderr_file -Raw -ErrorAction SilentlyContinue) } else { "" }
    if (-not $stderr_text) { $stderr_text = "" }
    # Combined для маркеров, которые исторически проверяют любой поток
    # (EXPECT_RUNTIME_PANIC: panic пишет в stderr через nv_panic, но
    # технически совместимы с любыми источниками; берём оба для
    # резистентности).
    $combined_text = "$stdout_text $stderr_text"

    # D89: runtime-маркеры обрабатываются после run.
    if ($expect_kind -eq 'runtime_panic') {
        # Ожидается ненулевой exit + panic-сообщение в любом потоке
        # (panic пишет в stderr, но проверяем combined на устойчивость).
        if ($run_exit -eq 0) {
            $results += [PSCustomObject]@{
                Name=$display; Status="NEG-NO-PANIC";
                Detail="expected `// EXPECT_RUNTIME_PANIC $expect_arg` but exe succeeded (exit=0)"
            }
            $fail++; return
        }
        if ($combined_text -notmatch [regex]::Escape($expect_arg)) {
            $snippet = if ($combined_text.Length -gt 150) { $combined_text.Substring(0,150) } else { $combined_text }
            $results += [PSCustomObject]@{
                Name=$display; Status="NEG-WRONG-PANIC";
                Detail="expected panic pattern '$expect_arg' not found in: $snippet"
            }
            $fail++; return
        }
        $results += [PSCustomObject]@{Name=$display; Status="PASS"; Detail="(runtime-panic)"}
        $pass++; return
    }
    if ($expect_kind -eq 'exit_code') {
        # Ожидается exact exit code = $expect_arg.
        if ($run_exit -ne $expect_arg) {
            $results += [PSCustomObject]@{
                Name=$display; Status="NEG-WRONG-EXIT";
                Detail="expected exit code $expect_arg, got $run_exit"
            }
            $fail++; return
        }
        $results += [PSCustomObject]@{Name=$display; Status="PASS"; Detail="(exit-code $expect_arg)"}
        $pass++; return
    }
    if ($expect_kind -eq 'stdout') {
        # Любой exit code OK; ожидается substring ТОЛЬКО в stdout (не stderr).
        if ($stdout_text -notmatch [regex]::Escape($expect_arg)) {
            $snippet = if ($stdout_text.Length -gt 150) { $stdout_text.Substring(0,150) } else { $stdout_text }
            $results += [PSCustomObject]@{
                Name=$display; Status="NEG-WRONG-STDOUT";
                Detail="expected stdout pattern '$expect_arg' not found in stdout: $snippet"
            }
            $fail++; return
        }
        $results += [PSCustomObject]@{Name=$display; Status="PASS"; Detail="(stdout)"}
        $pass++; return
    }
    if ($expect_kind -eq 'stderr') {
        # Любой exit code OK; ожидается substring ТОЛЬКО в stderr (не stdout).
        if ($stderr_text -notmatch [regex]::Escape($expect_arg)) {
            $snippet = if ($stderr_text.Length -gt 150) { $stderr_text.Substring(0,150) } else { $stderr_text }
            $results += [PSCustomObject]@{
                Name=$display; Status="NEG-WRONG-STDERR";
                Detail="expected stderr pattern '$expect_arg' not found in stderr: $snippet"
            }
            $fail++; return
        }
        $results += [PSCustomObject]@{Name=$display; Status="PASS"; Detail="(stderr)"}
        $pass++; return
    }

    # Default path: ожидается успешный run (exit code 0).
    if ($run_exit -ne 0) {
        # Для RUN-FAIL диагностики берём combined-вывод (stdout + stderr).
        $detail = ($combined_text -split "[\r\n]" | Where-Object { $_ } | Select-Object -Last 3) -join " | "
        $results += [PSCustomObject]@{Name=$display; Status="RUN-FAIL"; Detail=$detail}
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
