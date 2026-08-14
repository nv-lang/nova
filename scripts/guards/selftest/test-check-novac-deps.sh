#!/bin/sh
# Самотест check-novac-deps.sh — оба направления.
export LC_ALL=C
G="$(cd "$(dirname "$0")/.." && pwd)/check-novac-deps.sh"
ROOT="$(cd "$(dirname "$0")/../../.." && pwd)"
T="${TMPDIR:-/tmp}/novac-deps-selftest.$$"
mkdir -p "$T/src/lex" "$T/src/parse" "$T/src/rogue"
fails=0
ok() { echo "  ok: $1"; }
bad() { echo "  FAIL: $1" >&2; fails=$((fails+1)); }

cat > "$T/arch.md" <<'EOF'
## 3. Рёбра — с колонкой «что течёт»
| из | в | что течёт |
|---|---|---|
| `lex` | `source` | токены |
| `parse` | `lex`, `tree` | строит |
## 4. Дальше
EOF

# 1. Законно: parse импортирует lex (ребро есть).
echo "import ../lex.{lex}" > "$T/src/parse/parse.nv"
sh "$G" "$ROOT" "$T/src" "$T/arch.md" >/dev/null 2>&1 && ok "разрешённое ребро проходит" || bad "разрешённое покраснело"
sh "$G" "$ROOT" "$T/src" "$T/arch.md" 2>/dev/null | grep -q 'ok:' && ok "печатает ok:" || bad "нет ok: (№645)"

# 2. Нарушение: lex импортирует parse (ребра нет).
echo "import ../parse.{p}" > "$T/src/lex/lex.nv"
sh "$G" "$ROOT" "$T/src" "$T/arch.md" >/dev/null 2>&1 && bad "запрещённое ребро прошло" || ok "запрещённое ребро поймано"
rm "$T/src/lex/lex.nv"

# 3. Нарушение: модуль вне карты.
echo "// пусто" > "$T/src/rogue/r.nv"
sh "$G" "$ROOT" "$T/src" "$T/arch.md" >/dev/null 2>&1 && bad "модуль вне карты прошёл" || ok "модуль вне карты пойман"
rm -rf "$T/src/rogue"

# 4а. Нарушение: корневой main с ./-импортом вне таблицы (слепота поймана 2026-08-14).
echo "import ./lex.{lex}" > "$T/src/main.nv"
sh "$G" "$ROOT" "$T/src" "$T/arch.md" >/dev/null 2>&1 && bad "./-импорт main вне таблицы прошёл" || ok "./-импорт main вне таблицы пойман"
rm "$T/src/main.nv"

# 4. Законно: нет novac/src вовсе — «судить нечего», зелёный.
sh "$G" "$ROOT" "$T/absent" "$T/arch.md" >/dev/null 2>&1 && ok "отсутствие дерева — зелёное «судить нечего»" || bad "отсутствие дерева покраснело"

rm -rf "$T"
if [ "$fails" -eq 0 ]; then
    echo "test-check-novac-deps ok: 5/5"
    exit 0
fi
echo "test-check-novac-deps: FAIL ($fails)" >&2
exit 1
