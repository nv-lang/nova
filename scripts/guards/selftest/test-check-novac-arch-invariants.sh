#!/bin/sh
# Самотест check-novac-arch-invariants.py — оба направления.
export LC_ALL=C
G="$(cd "$(dirname "$0")/.." && pwd)/check-novac-arch-invariants.py"
ROOT="$(cd "$(dirname "$0")/../../.." && pwd)"
T="${TMPDIR:-/tmp}/novac-inv-selftest.$$"
mkdir -p "$T"
fails=0
CASES=0
ok() { CASES=$((CASES+1)); echo "  ok: $1"; }
bad() { CASES=$((CASES+1)); echo "  FAIL: $1" >&2; fails=$((fails+1)); }

# 1. Законный: разделы карты со счётчиками — зелено, со строкой ok:.
cat > "$T/good.md" <<'EOF'
## 1. Слои
текст. Счётчик: **1**.
## 4а. Идентичность
текст. Счётчик: **2**.
## 11. Сканеры
без счётчика — не подсуден.
EOF
python "$G" "$ROOT" "$T/good.md" >/dev/null 2>&1 && ok "карта со счётчиками проходит" || bad "законное покраснело"
python "$G" "$ROOT" "$T/good.md" 2>/dev/null | grep -q 'ok:' && ok "печатает ok:" || bad "нет ok: (№645)"

# 2. Нарушение: раздел карты без счётчика — красный.
cat > "$T/bad.md" <<'EOF'
## 1. Слои
текст. Счётчик: **1**.
## 3. Рёбра
инварианты прозой, счёта нет.
EOF
python "$G" "$ROOT" "$T/bad.md" >/dev/null 2>&1 && bad "раздел без счётчика прошёл" || ok "раздел без счётчика пойман"

# 3. Нарушение: ПОСЛЕДНИЙ раздел карты без счётчика (граница EOF) — красный.
cat > "$T/last.md" <<'EOF'
## 9. Кодоген
текст. Счётчик: **2**.
## 10. Атомики
последний и без счёта.
EOF
python "$G" "$ROOT" "$T/last.md" >/dev/null 2>&1 && bad "последний без счётчика прошёл" || ok "последний без счётчика пойман"

rm -rf "$T"
if [ "$fails" -eq 0 ]; then
    echo "test-check-novac-arch-invariants ok: $CASES/$CASES"
    exit 0
fi
echo "test-check-novac-arch-invariants: FAIL ($fails)" >&2
exit 1
