#!/bin/sh
# ПРОБА C — check-gate-steps-assert.sh
#
# ОБЕЩАНИЕ ШАПКИ (scripts/guards/check-gate-steps-assert.sh:3, дословно):
#   "Шаг гейта обязан предъявить свою строку, а не только код возврата."
#   "2. каждый страж зовётся из гейта через обёртку `guard`, а не голым
#       `bash`/`python`: обёртка требует в выводе строку `ok:`;"
#   "# --- 4. обёртка стоит в позиции аргумента чужой команды ---"
# СТРОКА ВЕРДИКТА:
#   "check-gate-steps-assert ok: каждый шаг гейта требует от стража
#    его собственную строку"
#
# ЧТО ПРОВЕРЯЕТ НА ДЕЛЕ:
#   GATE="$ROOT/scripts/gate.sh"          <- ОДИН файл
#   свойства 2 и 4 грепают ТОЛЬКО "$GATE".
# В дереве гейта ДВА: scripts/gate.sh и scripts/gate-novac.sh, и CI запускает
# ОБА (.github/workflows/nova-gate.yml:197 -- `bash scripts/gate-novac.sh`,
# :266 и :334 -- `bash scripts/gate.sh`). Свойство 1 (литеральный «слэш+n»)
# ЗНАЕТ про scripts/*.sh целиком, свойства 2 и 4 — нет.
# Ни один другой страж gate-novac.sh на эти два свойства не судит: его читают
# check-gate-budget.py (секунды), check-gate-timeout-word.py (пределы),
# check-novac-seams-listed.py (SEAMS), check-novac-guard-registry.py (имена),
# check-merge-discipline.sh, check-novac-fuzz-zero-panic.sh.
#
# ЗАПУСК:  sh cmd.sh [КОРЕНЬ-РЕПОЗИТОРИЯ]
set -u
HERE=$(cd "$(dirname "$0")" && pwd)
REPO="${1:-}"
if [ -z "$REPO" ]; then
    d="$HERE"
    while [ "$d" != "/" ] && [ ! -f "$d/scripts/guards/check-gate-steps-assert.sh" ]; do
        d=$(dirname "$d")
    done
    REPO="$d"
fi
G="$REPO/scripts/guards/check-gate-steps-assert.sh"
[ -f "$G" ] || { echo "cmd.sh: не нашёл корень репозитория; передай его аргументом" >&2; exit 2; }

T="${TMPDIR:-/tmp}/probeC.$$"
rm -rf "$T"; mkdir -p "$T/scripts/guards"

# один законный страж, чтобы свойство 3 (наличие строки ok:) молчало
cat > "$T/scripts/guards/check-demo.sh" <<'EOF'
#!/bin/sh
echo "check-demo ok: nothing"
EOF

CLEAN='#!/bin/sh
guard() { "$@"; }
guard "$ROOT/scripts/guards/check-demo.sh" "$ROOT" || fail "demo"
'
DIRTY_RAW='#!/bin/sh
guard() { "$@"; }
bash "$ROOT/scripts/guards/check-demo.sh" "$ROOT" || fail "demo"
'
DIRTY_ARG='#!/bin/sh
guard() { "$@"; }
timeout 60 \
    guard "$ROOT/scripts/guards/check-demo.sh" "$ROOT" || fail "demo"
'

run() {
    printf '%s\n' "--- $1 ---"
    bash "$G" "$T" 2>&1
    printf 'rc=%s\n\n' "$?"
}

printf '%s' "$CLEAN" > "$T/scripts/gate.sh"
printf '%s' "$CLEAN" > "$T/scripts/gate-novac.sh"
run "КОНТРОЛЬ 0: оба гейта чисты -> ожидается зелёный"

printf '%s' "$DIRTY_RAW" > "$T/scripts/gate.sh"
printf '%s' "$CLEAN"     > "$T/scripts/gate-novac.sh"
run "КОНТРОЛЬ 1: голый bash-вызов стража В gate.sh -> ожидается КРАСНЫЙ"

printf '%s' "$CLEAN"     > "$T/scripts/gate.sh"
printf '%s' "$DIRTY_RAW" > "$T/scripts/gate-novac.sh"
run "ДЕФЕКТ 1: ТОТ ЖЕ голый bash-вызов, перенесён в gate-novac.sh"

printf '%s' "$DIRTY_ARG" > "$T/scripts/gate.sh"
printf '%s' "$CLEAN"     > "$T/scripts/gate-novac.sh"
run "КОНТРОЛЬ 2: обёртка guard аргументом timeout В gate.sh -> ожидается КРАСНЫЙ"

printf '%s' "$CLEAN"     > "$T/scripts/gate.sh"
printf '%s' "$DIRTY_ARG" > "$T/scripts/gate-novac.sh"
run "ДЕФЕКТ 2: ТА ЖЕ форма, перенесена в gate-novac.sh"

printf '%s' "$CLEAN" > "$T/scripts/gate-novac.sh"
rm -f "$T/scripts/gate.sh"
run "КОНТРОЛЬ 3: gate.sh удалён, gate-novac.sh на месте -> страж отказывает («нет gate.sh»), то есть про второй гейт он не знает вовсе"

rm -rf "$T"
