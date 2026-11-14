#!/usr/bin/env bash
# scripts/guards/check-packages-sweep.sh
# Основание: реестр 221.1 №1964.
# Спутниковые пакеты собираются и проходят тесты тулчейном ЭТОЙ монорепы.
#
# ДОМ И ОСНОВАНИЕ: релиз oracle-v0.1.3 (2026-11-08) проверялся на примерах,
# conformance, std и одном внешнем продукте — но НЕ на спутниковых пакетах
# (nova-http/tls/compress/bignum/polaris/…); владелец спросил «а ты что не
# проверяешь новую версию компилятора на всех пакетах Нова?» — и был прав:
# пакеты — первые потребители тулчейна. Страж закрывает класс: соседние
# чекауты /../nova-*/ с nova.toml прогоняются `nova test src` бинарём
# ЭТОГО дерева (env-рецепт pins std/rt/gc на монорепу — как smoke
# claude-limits).
#
# ДЕГРАДАЦИЯ (осознанная): соседних чекаутов может не быть (CI клонирует
# только монорепу) — страж печатает «судить нечего» и зелёнеет НЕ молча:
# список проверенных и пропущенных пакетов всегда виден в выводе. Полный
# локальный гейт и релизный чек-лист получают полный свип.
#
# Запуск: scripts/guards/check-packages-sweep.sh <корень репо> [соседия]
#   <корень>   — корень монорепы (бинарь nova-cli/target/release/nova);
#   [соседия]  — каталог с чекаутами пакетов (по умолчанию родитель корня).
set -uo pipefail
ROOT="${1:-.}"
PKGS_ROOT="${2:-$(cd "$ROOT/.." && pwd)}"
cd "$ROOT" || { echo "check-packages-sweep: FAIL — нет корня $ROOT" >&2; exit 1; }

BIN="$ROOT/nova-cli/target/release/nova.exe"
[ -x "$BIN" ] || BIN="$ROOT/nova-cli/target/release/nova"
if [ ! -x "$BIN" ]; then
    echo "check-packages-sweep: FAIL — нет собранного nova (nova-cli/target/release/nova); свип пакетов требует бинарь" >&2
    exit 1
fi

# Env-рецепт: пакеты не тянут собственный тулчейн — std/rt/gc от монорепы
# (тот же контракт, что у claude-limits: рантайм-архивы кэшируются в $ROOT/target).
export NOVA_CG_INCLUDE="$ROOT/compiler-codegen"
export NOVA_STD_PATH="$ROOT/std"
export NOVA_RT_DIR="$ROOT/compiler-codegen/nova_rt"
if [ -d "$ROOT/target/gc-cache" ]; then
    export NOVA_GC_LIB_DIR="$ROOT/target/gc-cache"
    export NOVA_GC_INCLUDE_DIR="$ROOT/target/gc-cache/include"
fi

checked=0; skipped=0; failed=0
for toml in "$PKGS_ROOT"/nova-*/nova.toml; do
    [ -f "$toml" ] || continue
    pkg_dir=$(dirname "$toml")
    pkg=$(basename "$pkg_dir")
    if [ "$pkg" = "nova-cairo" ]; then
        echo "  SKIP $pkg (пакет не завершён, исключён из релизного свипа по решению владельца)"
        skipped=$((skipped+1))
        continue
    fi
    # src/ — канон пакетов; без него тестить нечего (пропуск с печатью).
    if [ ! -d "$pkg_dir/src" ]; then
        echo "  SKIP $pkg (нет src/)"
        skipped=$((skipped+1)); continue
    fi
    rc=0
    # nova-polaris (2026-11-10): ws_upgrade needs a real-socket deadline - the
    # suite is 50/50 PASS at --timeout 600; the runner default (116s) killed
    # it under full-suite contention, which read as a package FAIL.
    out=$(cd "$pkg_dir" && timeout 900 "$BIN" test src --timeout 600 2>&1) || rc=$?
    if [ "$rc" -eq 124 ]; then
        echo "  FAIL $pkg (СНЯТ ПРЕДЕЛОМ 900с: вердикта нет — тест-раннер убит, предмет не судился)" >&2
        failed=$((failed+1)); continue
    fi
    line=$(printf '%s\n' "$out" | grep -aE '^PASS: ' | tail -1)
    if [ "$rc" -eq 0 ] && [[ "$line" =~ ^PASS:[[:space:]]+[1-9][0-9]*[[:space:]]+FAIL:[[:space:]]+0([[:space:]]|$) ]]; then
        echo "  ok   $pkg ($line)"
        checked=$((checked+1))
    else
        echo "  FAIL $pkg (exit $rc; ${line:-нет PASS-строки}; хвост вывода ниже)" >&2
        printf '%s\n' "$out" | grep -aE '^(CC-FAIL|CODEGEN-FAIL|RUN-FAIL|TIMEOUT|NEG-[A-Z-]+)' | head -3 >&2
        failed=$((failed+1))
    fi
done

echo "check-packages-sweep: проверено $checked, пропущено $skipped, отказов $failed"
[ "$failed" -eq 0 ] || { echo "check-packages-sweep: FAIL — пакеты падают на этом тулчейне" >&2; exit 1; }
if [ "$checked" -eq 0 ]; then
    echo "check-packages-sweep ok (деградация): соседних пакетов под рукой нет — судить нечего; полный свип — в локальном полном гейте" >&2
    exit 0
fi
echo "check-packages-sweep ok: все проверенные пакеты зелёные на этом тулчейне"
