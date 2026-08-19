#!/bin/sh
# Самотест check-novac-deps.py — оба направления.
export LC_ALL=C
G="$(cd "$(dirname "$0")/.." && pwd)/check-novac-deps.py"
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
python "$G" "$ROOT" "$T/src" "$T/arch.md" >/dev/null 2>&1 && ok "разрешённое ребро проходит" || bad "разрешённое покраснело"
python "$G" "$ROOT" "$T/src" "$T/arch.md" 2>/dev/null | grep -q 'ok:' && ok "печатает ok:" || bad "нет ok: (№645)"

# 2. Нарушение: lex импортирует parse (ребра нет).
echo "import ../parse.{p}" > "$T/src/lex/lex.nv"
python "$G" "$ROOT" "$T/src" "$T/arch.md" >/dev/null 2>&1 && bad "запрещённое ребро прошло" || ok "запрещённое ребро поймано"
rm "$T/src/lex/lex.nv"

# 3. Нарушение: модуль вне карты.
echo "// пусто" > "$T/src/rogue/r.nv"
python "$G" "$ROOT" "$T/src" "$T/arch.md" >/dev/null 2>&1 && bad "модуль вне карты прошёл" || ok "модуль вне карты пойман"
rm -rf "$T/src/rogue"

# 4а. Нарушение: корневой main с ./-импортом вне таблицы (слепота поймана 2026-08-14).
echo "import ./lex.{lex}" > "$T/src/main.nv"
python "$G" "$ROOT" "$T/src" "$T/arch.md" >/dev/null 2>&1 && bad "./-импорт main вне таблицы прошёл" || ok "./-импорт main вне таблицы пойман"
rm "$T/src/main.nv"

# 5. Нарушение: импорт с ОТСТУПОМ (слепота якоря ^import, найдена
#    адверсарной проверкой 2026-08-17 — проба собиралась оракулом, то есть
#    единственное архитектурное правило обходилось двумя пробелами).
printf '  import ../parse.{p}
' > "$T/src/lex/lex.nv"
python "$G" "$ROOT" "$T/src" "$T/arch.md" >/dev/null 2>&1 && bad "импорт с отступом вне таблицы прошёл" || ok "импорт с отступом судится"
rm -f "$T/src/lex/lex.nv"

# 7. Форма `use` — ровно такой же ввоз имён, как import (проба 2026-08-17:
#    `use ../lex.{TokenKind}` в diag.nv собралась оракулом и типизировалась,
#    то есть ребро рабочее). Страж видел только `import`, и единственное
#    архитектурное правило обходилось сменой ключевого слова.
printf 'use ../parse.{p}\n' > "$T/src/lex/lex.nv"
python "$G" "$ROOT" "$T/src" "$T/arch.md" >/dev/null 2>&1 && bad "use-форма вне таблицы прошла" || ok "use судится наравне с import"
printf 'export import ../parse.{p}\n' > "$T/src/lex/lex.nv"
python "$G" "$ROOT" "$T/src" "$T/arch.md" >/dev/null 2>&1 && bad "export import вне таблицы прошёл" || ok "export import судится"
rm -f "$T/src/lex/lex.nv"

# 6. Нарушение: ЦИКЛ в самой таблице. До 2026-08-17 ацикличность считалась
#    «следствием таблицы» и не проверялась ничем: цикл добавлялся строкой
#    ровно так же, как честное ребро.
# Строка цикла обязана лечь ВНУТРЬ раздела §3: awk обрывает таблицу на
# следующем "## ", и дописанная в конец файла строка не считается ребром
# вовсе (на этом самотест едва не «прошёл» по неверной причине).
awk '/^## 4\./ { print "| `lex` | `parse` | cycle |" } { print }' "$T/arch.md" > "$T/arch-cycle.md"
CYCOUT=$(python "$G" "$ROOT" "$T/src" "$T/arch-cycle.md" 2>&1)
if printf '%s' "$CYCOUT" | grep -q "ЦИКЛ"; then ok "цикл в таблице краснеет ПО ПРИЧИНЕ цикла"; else bad "цикл не назван причиной: $CYCOUT"; fi
rm -f "$T/arch-cycle.md"


# 4. Законно: нет novac/src вовсе — «судить нечего», зелёный.
python "$G" "$ROOT" "$T/absent" "$T/arch.md" >/dev/null 2>&1 && ok "отсутствие дерева — зелёное «судить нечего»" || bad "отсутствие дерева покраснело"

rm -rf "$T"
if [ "$fails" -eq 0 ]; then
    echo "test-check-novac-deps ok: 9/9"
    exit 0
fi
echo "test-check-novac-deps: FAIL ($fails)" >&2
exit 1
