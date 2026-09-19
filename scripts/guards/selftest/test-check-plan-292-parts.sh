#!/usr/bin/env bash
# Селфтест scripts/guards/check-plan-292-parts.sh.
#
# Обе стороны, и к ним третья, без которой первые две ничего не стоят: страж
# обязан краснеть на ПУСТОЙ МИШЕНИ. Перечень частей живёт в таблице; перепиши
# раздел другой формой — и страж зеленел бы, не проверив ни одного пути,
# рапортуя «нарушений ноль» о том, чего не смотрел.
set -u
export LC_ALL=C

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
G="$ROOT/scripts/guards/check-plan-292-parts.sh"
FAILED=0
OKN=0
ok()  { OKN=$((OKN + 1)); echo "  ok: $1"; }
bad() { echo "  ПРОВАЛ: $1" >&2; FAILED=1; }

TMP=$(mktemp -d); trap 'rm -rf "$TMP"' EXIT
mk() {
    rm -rf "$TMP/docs" "$TMP/scripts"
    mkdir -p "$TMP/docs/plans" "$TMP/scripts/tools"
}
plan() { cat > "$TMP/docs/plans/292-stop-requires-proof.md"; }

# 1. все названные части на месте — зелено.
mk
: > "$TMP/scripts/tools/alpha.py"
plan <<'EOF'
| часть | что делает | без неё |
|---|---|---|
| `scripts/tools/alpha.py` | делает дело | беда |
EOF
out=$(bash "$G" "$TMP" 2>&1); rc=$?
if [ "$rc" -eq 0 ]; then ok "все части на месте — зелено"; else bad "ложный отказ: $out"; fi

# 2. часть переименована — красно, и названа поимённо.
mk
: > "$TMP/scripts/tools/alpha-renamed.py"
plan <<'EOF'
| часть | что делает | без неё |
|---|---|---|
| `scripts/tools/alpha.py` | делает дело | беда |
EOF
out=$(bash "$G" "$TMP" 2>&1); rc=$?
if [ "$rc" -eq 1 ] && echo "$out" | grep -q "alpha.py"; then
    ok "пропавшая часть — красно, и названа"
else
    bad "не поймал пропавшую часть (код $rc): $out"
fi

# 3. ПУСТАЯ МИШЕНЬ: таблицы нет — красно, а не «нарушений ноль».
mk
plan <<'EOF'
# план без таблицы частей

Проза, в которой путей нет вовсе.
EOF
out=$(bash "$G" "$TMP" 2>&1); rc=$?
if [ "$rc" -eq 1 ] && echo "$out" | grep -q "таблиц"; then
    ok "пустая мишень — красно, а не тихое «ноль»"
else
    bad "на пустой мишени страж смолчал (код $rc): $out"
fi

# 4. слэш-команда в таблице путём НЕ считается (иначе страж красит верную запись).
mk
: > "$TMP/scripts/tools/alpha.py"
plan <<'EOF'
| часть | что делает | без неё |
|---|---|---|
| `scripts/tools/alpha.py` | делает дело | беда |
| `/status` | показывает побеги | не видно |
EOF
out=$(bash "$G" "$TMP" 2>&1); rc=$?
if [ "$rc" -eq 0 ]; then ok "слэш-команда не принята за путь"; else bad "покраснел на команде: $out"; fi

# 5. файла плана нет — беда САМОГО стража, и она громкая.
mk
out=$(bash "$G" "$TMP" 2>&1); rc=$?
if [ "$rc" -eq 1 ] && echo "$out" | grep -q "нет файла плана"; then
    ok "нет плана — громкий отказ, а не зелёный вердикт"
else
    bad "отсутствие плана прошло тихо (код $rc): $out"
fi

if [ "$FAILED" -eq 0 ]; then
    echo "test-check-plan-292-parts ok: $OKN/$OKN"
    exit 0
fi
echo "test-check-plan-292-parts FAIL"
exit 1
