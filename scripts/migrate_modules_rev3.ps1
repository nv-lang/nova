# Plan 42 Sub-plan 42.6 — миграция module declarations rev-1 → rev-3 (parent.X).
#
# Walks все *.nv файлы в указанных package roots, для каждого считает
# expected rev-3 declaration `parent.target` и переписывает первую строку
# `module <full-path>` на `module <parent>.<target>`, если файл сейчас
# в legacy rev-1 формате.
#
# Folder-module peers уже в rev-3 (declare folder-name) — walker их пропускает.
#
# Usage:
#   pwsh scripts/migrate_modules_rev3.ps1 -DryRun       # preview changes
#   pwsh scripts/migrate_modules_rev3.ps1               # apply

param(
    [switch]$DryRun
)

$ErrorActionPreference = "Stop"

# Workspace members с своим nova.toml. Hardcoded — bootstrap-script, не
# general tool.
$Members = @(
    @{ Path = "std";        Package = "std" },
    @{ Path = "nova_tests"; Package = "nova_tests" }
)

$repoRoot = Resolve-Path "$PSScriptRoot\.."
$totalChecked = 0
$totalMigrated = 0
$totalSkipped = 0
$totalErrors = 0

foreach ($member in $Members) {
    $sourceRoot = Join-Path $repoRoot $member.Path
    $package = $member.Package
    if (-not (Test-Path $sourceRoot)) {
        Write-Host "[skip] $sourceRoot not found"
        continue
    }
    Write-Host "=== $($member.Path) (package=$package) ==="

    $files = Get-ChildItem -Path $sourceRoot -Recurse -Filter "*.nv" -File
    foreach ($file in $files) {
        $totalChecked++
        $relPath = $file.FullName.Substring($sourceRoot.ToString().Length + 1) -replace '\\', '/'
        # parts = path components without .nv extension
        $rel = $relPath -replace '\.nv$', ''
        $parts = $rel -split '/'

        if ($parts.Length -eq 0) {
            $totalSkipped++
            continue
        }

        # expected rev-1 = "<package>.<parts joined by .>"
        $expectedRev1 = ($package + "." + ($parts -join "."))

        # expected rev-3 = "<parent>.<target>". parent = parts[-2] OR package
        # (если file at root). target = parts[-1].
        $target = $parts[-1]
        if ($parts.Length -eq 1) {
            $parent = $package
        } else {
            $parent = $parts[-2]
        }
        $expectedRev3 = "$parent.$target"

        # Read file, find first `module ...` line.
        $lines = Get-Content -LiteralPath $file.FullName -Encoding UTF8
        $moduleLineIdx = -1
        $declared = $null
        for ($i = 0; $i -lt $lines.Length; $i++) {
            $line = $lines[$i]
            if ($line -match '^\s*module\s+(\S+)\s*$') {
                $moduleLineIdx = $i
                $declared = $matches[1]
                break
            }
        }

        if ($moduleLineIdx -lt 0) {
            Write-Host "  [no-module] $relPath"
            $totalSkipped++
            continue
        }

        if ($declared -eq $expectedRev3) {
            # Already rev-3 (single-file at root: rev-3 == rev-1).
            $totalSkipped++
            continue
        }

        if ($declared -eq $expectedRev1) {
            # rev-1 → rev-3 migration needed.
            Write-Host "  [migrate] $relPath : $declared → $expectedRev3"
            if (-not $DryRun) {
                $lines[$moduleLineIdx] = "module $expectedRev3"
                # Preserve original line endings (use UTF8 no BOM).
                $content = ($lines -join "`n") + "`n"
                [System.IO.File]::WriteAllText($file.FullName, $content, [System.Text.UTF8Encoding]::new($false))
            }
            $totalMigrated++
            continue
        }

        # Declared не matches ни rev-1 ни rev-3 single-file. Возможно
        # folder-module peer (declared = folder name) или нестандарт.
        Write-Host "  [skip-other] $relPath : declares '$declared' (expected rev-3 '$expectedRev3' or rev-1 '$expectedRev1')"
        $totalSkipped++
    }
}

Write-Host ""
Write-Host "=== Summary ==="
Write-Host "  Checked:  $totalChecked"
Write-Host "  Migrated: $totalMigrated"
Write-Host "  Skipped:  $totalSkipped"
Write-Host "  Errors:   $totalErrors"
if ($DryRun) {
    Write-Host "  (DryRun - no files modified)"
}
