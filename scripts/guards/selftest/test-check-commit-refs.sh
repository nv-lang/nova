#!/usr/bin/env bash
# Самотест scripts/guards/check-commit-refs.sh — обе стороны.
#
# Страж без самотеста доказывает только то, что он запускается. Здесь
# проверяется и что он ЛОВИТ нарушение, и что он МОЛЧИТ на законной форме —
# вторая половина важнее: страж, красящий всё подряд, снимается первым.
#
# Достижимость считается по настоящей репе (иначе нечем), а текст берётся из
# временной фикстуры — ради этого у ядра и стража два корня, а не один.

set -u
export LC_ALL=C

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
GUARD="$ROOT/scripts/guards/check-commit-refs.sh"
CORE="$ROOT/scripts/guards/commit-refs-scan.py"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

PASS=0; FAIL=0
ok()   { PASS=$((PASS+1)); echo "  ok   $1"; }
bad()  { FAIL=$((FAIL+1)); echo "  FAIL $1" >&2; }
check(){ if [ "$2" = "$3" ]; then ok "$1"; else bad "$1 (ждал '$3', получил '$2')"; fi; }

LIVE="$(git -C "$ROOT" rev-parse --short=11 HEAD)"
# №942-сосед: хеш, живущий ТОЛЬКО на СОСЕДНЕЙ ветке (не main, не HEAD).
# До 2026-09-05 такой считался мёртвым, хотя лежит в этой же репе:
# реестр №894 ссылался на коммит окна 274, и гейт краснел за живой хеш.
# Строится так: пустой коммит во ВРЕМЕННОЙ ветке реальной репы,
# без переключения HEAD и без единой правки в рабочем дереве.
SIDE_BRANCH="selftest/commit-refs-$$"
SIDE="$(git -C "$ROOT" commit-tree "$(git -C "$ROOT" rev-parse HEAD^{tree})" -p HEAD -m "selftest side commit (commit-refs)" 2>/dev/null || true)"
if [ -n "$SIDE" ]; then
    git -C "$ROOT" branch -f "$SIDE_BRANCH" "$SIDE" >/dev/null 2>&1
    SIDE="$(git -C "$ROOT" rev-parse --short=11 "$SIDE")"
    trap 'git -C "$ROOT" branch -D "$SIDE_BRANCH" >/dev/null 2>&1; rm -rf "$TMP"' EXIT
fi

FIX="$TMP/fix"
mkdir -p "$FIX/docs/plans"
cat > "$FIX/docs/plans/probe.md" <<EOF
1 мёртвый хеш: \`deadbee1234\`
2 живой хеш: \`$LIVE\`
3 десятичное число, не хеш: \`2884744404960\`
4 https://github.com/nv-lang/nova/commit/1234567890a без темы и даты
5 https://github.com/nv-lang/nova/commit/1234567890a «тема коммита такая», 2026-08-12
6 см. https://gitverse.ru/nv-lang/nova/blob/main/README.md
7 | GitVerse | https://gitverse.ru/nv-lang | mirror |
8 хеш соседней ветки: \`$SIDE\`
9 id сессии с приставкой рода: \`session:deadbee1234\`
10 id задания CronCreate: \`task:b0b843e5\`
11 tagged-template Nova, не ссылка: ro b = bytes\`deadbeef\`
12 мёртвый хеш рядом со словом «сессия» и «handoff» — всё равно хеш: \`deadbee5678\`
EOF

OUT="$(python "$CORE" "$ROOT" "$FIX")"
line() { printf '%s\n' "$OUT" | grep -c "^$1|docs/plans/probe.md:$2|" || true; }

echo "== ядро: ловит =="
check "мёртвый хеш даёт R2"                  "$(line R2 1)" "1"
# КОНТРОЛЬ ПРОТИВ ОСЛАБЛЕНИЯ (реестр №1110). Первая предложенная починка —
# «освобождать hex, если рядом слова сессия/передача/handoff». Замер
# 2026-09-15 её отверг: таких строк в доке 48, и почти все несут НАСТОЯЩИЕ
# хеши («landed (commit `5307ddfdbf3`) — Session 2 checkpoint»). Эта строка
# держит отказ: слова рядом есть, хеш мёртв — и он обязан считаться.
check "слова про сессию не освобождают мёртвый хеш" "$(line R2 12)" "1"
check "ссылка на коммит без темы даёт R1"    "$(line R1 4)" "1"
check "ссылка через зеркало даёт R3"         "$(line R3 6)" "1"

echo "== ядро: молчит =="
check "живой хеш не трогает"                          "$(line R2 2)" "0"
check "десятичное число не хеш"                       "$(line R2 3)" "0"
check "ссылка на коммит с темой и датой проходит"     "$(line R1 5)" "0"
check "строка про сами зеркала проходит"              "$(line R3 7)" "0"
# Не-коммитный hex объявлен приставкой рода — предмет правила его не касается
# (реестр №1065, doc-conventions «Ссылки наружу»). Прежде эта форма не
# считалась СЛУЧАЙНО: двоеточие не hex, и шаблон мимо неё промахивался.
#
# ЧЕСТНО О СИЛЕ ЭТИХ ДВУХ СТРОК: это ПИНЫ, а не доказанная починка. Проба в
# обе стороны 2026-09-15 (возврат прежнего предиката) их НЕ покраснила —
# прежний шаблон их тоже не считал. Они держат форму от будущего расширения
# шаблона: тот, кто решит ловить hex с пунктуацией внутри апострофов (щит
# многоточия, приёмка №1065 п.2), увидит здесь, что приставочная форма должна
# остаться вне счёта. Зубы у правила растут на СТОРОНЕ ИСТОЧНИКА — в
# конвенции, а не здесь. Покраснела от возврата предиката ровно одна строка
# ниже — tagged-template.
check "id сессии с приставкой не считается"           "$(line R2 9)"  "0"
check "id задания с приставкой не считается"          "$(line R2 10)" "0"
check "tagged-template bytes\`…\` не ссылка на коммит" "$(line R2 11)" "0"
if [ -n "$SIDE" ]; then
    check "хеш соседней ветки не мёртв (№942)" "$(line R2 8)" "0"
else
    echo "  SKIP хеш соседней ветки: commit-tree недоступен" >&2
fi

echo "== страж: база =="
printf 'dead_hash_refs=0\ncommit_url_no_context=0\nmirror_links=0\n' > "$TMP/zero.baseline"
NOVA_COMMITREFS_BASELINE="$TMP/zero.baseline" bash "$GUARD" "$ROOT" "$FIX" >/dev/null 2>&1
check "выше базы — падает" "$?" "1"

# R2 в фикстуре ДВА: строка 1 (голый мёртвый хеш) и строка 12 (мёртвый хеш
# рядом со словами про сессию — контроль против ослабления). Число здесь
# держит форму «база = ровно текущий счёт»: на своей базе страж молчит.
printf 'dead_hash_refs=2\ncommit_url_no_context=1\nmirror_links=1\n' > "$TMP/one.baseline"
NOVA_COMMITREFS_BASELINE="$TMP/one.baseline" bash "$GUARD" "$ROOT" "$FIX" >/dev/null 2>&1
check "на базе — проходит" "$?" "0"

echo "== страж: молчание git не есть чистота (№645) =="
mkdir -p "$TMP/norepo"
NOVA_COMMITREFS_BASELINE="$TMP/one.baseline" bash "$GUARD" "$TMP/norepo" "$FIX" >/dev/null 2>&1
check "нет репы — FAIL, а не ok" "$?" "1"

echo "== страж: настоящее дерево на своей базе =="
bash "$GUARD" "$ROOT" >/dev/null 2>&1
check "дерево проекта зелёное" "$?" "0"

echo ""
echo "итог: $PASS ok, $FAIL FAIL"
[ "$FAIL" -eq 0 ] || exit 1
