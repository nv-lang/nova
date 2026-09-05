#!/usr/bin/env bash
# Самотест check-no-mojibake.sh.
#
# Образцы порчи задаются ЭКРАНИРОВАННЫМИ последовательностями через python, а не
# литералами в этом файле: сам файл исключён из скана, но литерал в нём всё
# равно прошёл бы через оболочку при любой правке — то есть проверка порчи сама
# бы её и получила. Тот же приём, что и в правиле, которое она стережёт.

set -u
export LC_ALL=C

DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
G="$DIR/check-no-mojibake.sh"
TMP="${TMPDIR:-/tmp}/selftest_mojibake_$$"
FAILED=0
ok()  { echo "  ok: $1"; }
bad() { echo "  ПРОВАЛ: $1" >&2; FAILED=1; }

# $1 — python-выражение с содержимым файла, $2 — база cp1251-подписи,
# $3 — база символа-замены (не задана → 0)
setup() {
    rm -rf "$TMP"; mkdir -p "$TMP/docs/dev" "$TMP/scripts/guards"
    cp "$DIR/mojibake-scan.py" "$TMP/scripts/guards/"
    python -c "
import io, sys
io.open(sys.argv[1], 'w', encoding='utf-8', newline='\n').write($1)
" "$TMP/docs/dev/probe.md"
    printf 'mojibake_lines=%s\n' "$2" > "$TMP/scripts/guards/mojibake.baseline"
    printf 'fffd_lines=%s\n' "${3:-0}" >> "$TMP/scripts/guards/mojibake.baseline"
}
trap 'rm -rf "$TMP"' EXIT

CLEAN="u'# Обычный русский текст с кавычками «так» и тире — всё чисто.\n'"
# «РћС‚РїСЂР°РІР»СЏР№» — реальный кусок порчи из происшествия 2026-08-13.
DIRTY="u'# РћСЂРїСЂРаРІР»СЏРй текст\n'"

# 1. Чистый русский текст — норма. Кавычки «» и тире не должны краснеть:
#    первая редакция признака ловила именно их (2468 ложных).
setup "$CLEAN" 0
out=$(bash "$G" "$TMP" 2>&1); rc=$?
if [ "$rc" -eq 0 ]; then ok "чистый русский текст с «» и тире проходит"; else bad "ложный отказ: $out"; fi

# 2. Настоящая порча — отказ.
setup "$DIRTY" 0
out=$(bash "$G" "$TMP" 2>&1); rc=$?
if [ "$rc" -eq 1 ] && echo "$out" | grep -q "probe.md"; then ok "ловит порчу и НАЗЫВАЕТ файл"; else bad "не поймал (rc=$rc): $out"; fi

# 3. Храповик: порча в пределах базы — зелено.
setup "$DIRTY" 1
out=$(bash "$G" "$TMP" 2>&1); rc=$?
if [ "$rc" -eq 0 ]; then ok "долг в пределах базы проходит"; else bad "храповик не пропускает базу: $out"; fi

# 4. Снижение — подсказка, а не отказ.
setup "$CLEAN" 5
out=$(bash "$G" "$TMP" 2>&1); rc=$?
if [ "$rc" -eq 0 ] && echo "$out" | grep -q "СНИЗИЛСЯ"; then ok "снижение — подсказка"; else bad "снижение обработано неверно (rc=$rc): $out"; fi

# 5. На настоящем дереве зелёный.
REAL="$(cd "$DIR/../.." && pwd)"
out=$(bash "$G" "$REAL" 2>&1); rc=$?
if [ "$rc" -eq 0 ]; then ok "на настоящем дереве зелёный"; else bad "красный на настоящем дереве: $out"; fi

# 6. Ремонтный инструмент НЕ краснит: он хранит образцы как данные.
if [ -f "$REAL/scripts/tools/demojibake.py" ]; then
    out=$(python "$DIR/mojibake-scan.py" "$REAL" 2>/dev/null | grep -c "demojibake.py" || true)
    if [ "${out:-0}" -eq 0 ]; then ok "ремонтный инструмент исключён"; else bad "страж краснит на demojibake.py"; fi
fi

# 7. Страж назван на странице правил.
if grep -q "check-no-mojibake.sh" "$REAL/docs/dev/rules-for-agents.md" 2>/dev/null; then
    ok "страж назван на странице правил"
else
    bad "страж не назван в docs/dev/rules-for-agents.md"
fi

# ===== ВТОРАЯ ПОДПИСЬ: СИМВОЛ-ЗАМЕНА U+FFFD (№972) =====
# Образец задаётся ЭКРАНИРОВАНИЕМ через python — по той же причине, что и
# образцы выше: литерал символа-замены в этом файле сам был бы порчей.
FFFD="u'# текст \ufffd потерян\n'"

# 8. Чистый текст при нулевой базе замен — норма.
setup "$CLEAN" 0 0
out=$(bash "$G" "$TMP" 2>&1); rc=$?
if [ "$rc" -eq 0 ]; then ok "чистый текст проходит и по второй подписи"; else bad "ложный отказ на второй подписи: $out"; fi

# 9. Символ-замена сверх базы — ОТКАЗ, с названием файла.
setup "$FFFD" 0 0
out=$(bash "$G" "$TMP" 2>&1); rc=$?
if [ "$rc" -eq 1 ] && echo "$out" | grep -q "probe.md"; then ok "ловит символ-замену и НАЗЫВАЕТ файл"; else bad "не поймал символ-замену (rc=$rc): $out"; fi

# 10. И советует ВЕРНОЕ лечение: demojibake здесь не помогает, нужна история git.
setup "$FFFD" 0 0
out=$(bash "$G" "$TMP" 2>&1)
if echo "$out" | grep -q "972"; then ok "отсылает к №972, а не к demojibake"; else bad "совет по символу-замене не назван: $out"; fi

# 11. Храповик второй метрики: в пределах базы — зелено.
setup "$FFFD" 0 1
out=$(bash "$G" "$TMP" 2>&1); rc=$?
if [ "$rc" -eq 0 ]; then ok "долг замен в пределах базы проходит"; else bad "второй храповик не пропускает базу: $out"; fi

# 12. Метрики НЕЗАВИСИМЫ: cp1251-порча не должна расходовать базу замен.
#     Без этого две подписи слились бы в один счёт — ровно то, чего правка
#     №972 избегала.
setup "$DIRTY" 0 5
out=$(bash "$G" "$TMP" 2>&1); rc=$?
if [ "$rc" -eq 1 ] && echo "$out" | grep -q "637"; then ok "cp1251-порча судится своей базой, а не базой замен"; else bad "метрики перепутаны (rc=$rc): $out"; fi

# ===== РАСШИРЕННЫЙ ОХВАТ И ФИЛЬТР ПО GIT (№948) =====

# 13. Порча в `.txt` — расширение, которого страж НЕ смотрел до 2026-09-05.
setup "$DIRTY" 0 0
mv "$TMP/docs/dev/probe.md" "$TMP/docs/dev/probe.txt"
out=$(bash "$G" "$TMP" 2>&1); rc=$?
if [ "$rc" -eq 1 ] && echo "$out" | grep -q "probe.txt"; then ok "ловит порчу в .txt (расширенный охват)"; else bad "не поймал в .txt (rc=$rc): $out"; fi

# 14. Порча в `.gitignore` — файл БЕЗ расширения, реальный носитель 2026-09-05.
setup "$DIRTY" 0 0
mv "$TMP/docs/dev/probe.md" "$TMP/.gitignore"
out=$(bash "$G" "$TMP" 2>&1); rc=$?
if [ "$rc" -eq 1 ] && echo "$out" | grep -q "gitignore"; then ok "ловит порчу в .gitignore"; else bad "не поймал в .gitignore (rc=$rc): $out"; fi

# 15/16. ФИЛЬТР ПО GIT в обе стороны. Внутри РЕПОЗИТОРИЯ судится только то, что
#     git отслеживает: иначе число стража зависит от того, гонял ли кто-то
#     тесты (415 сгенерированных `.c` в spec_tests, отслеживается ОДИН).
if git --version >/dev/null 2>&1; then
    setup "$DIRTY" 0 0
    ( cd "$TMP" && git init -q . && git config user.email t@t && git config user.name t ) >/dev/null 2>&1
    out=$(bash "$G" "$TMP" 2>&1); rc=$?
    if [ "$rc" -eq 0 ]; then ok "НЕотслеживаемый файл в репозитории не считается"; else bad "неотслеживаемый файл покраснел (rc=$rc): $out"; fi

    ( cd "$TMP" && git add docs/dev/probe.md ) >/dev/null 2>&1
    out=$(bash "$G" "$TMP" 2>&1); rc=$?
    if [ "$rc" -eq 1 ] && echo "$out" | grep -q "probe.md"; then ok "тот же файл ПОСЛЕ git add — считается"; else bad "отслеживаемый файл не пойман (rc=$rc): $out"; fi
else
    ok "git недоступен — проверка фильтра пропущена (и это НАЗВАНО)"
fi

if [ "$FAILED" -eq 0 ]; then echo "селфтест check-no-mojibake: все проверки ok"; exit 0; fi
echo "селфтест check-no-mojibake: ЕСТЬ ПРОВАЛЫ" >&2
exit 1
