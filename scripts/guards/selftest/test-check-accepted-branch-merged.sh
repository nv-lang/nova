#!/usr/bin/env bash
# Самотест check-accepted-branch-merged.sh.

set -u
export LC_ALL=C

G="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)/check-accepted-branch-merged.sh"
TMP="${TMPDIR:-/tmp}/selftest_abm_$$"
FAILED=0
# Число случаев в итоге печатает СЧЁТЧИК (check-selftest-honest-count, №989).
CASES=0
ok()  { CASES=$((CASES+1)); echo "  ok: $1"; }
bad() { CASES=$((CASES+1)); echo "  ПРОВАЛ: $1" >&2; FAILED=1; }

setup() {  # содержимое строки реестра
    rm -rf "$TMP"
    mkdir -p "$TMP/docs/plans"
    git -C "$TMP" init -q 2>/dev/null
    git -C "$TMP" -c user.name=t -c user.email=t@t commit -q --allow-empty -m base 2>/dev/null
    git -C "$TMP" branch -M main 2>/dev/null
    printf '%s\n' "$1" > "$TMP/docs/plans/221.1-bug-sweep.md"
}
trap 'rm -rf "$TMP"' EXIT

# 1. Ветки, названной в приёмке, не существует — норма: её удалили после слияния.
setup '| 1 | К1 | ЗАКРЫТО 2026-08-11 (окно p-ghost42). |'
out=$(bash "$G" "$TMP" 2>&1); rc=$?
if [ "$rc" -eq 0 ]; then ok "удалённая ветка не считается долгом"; else bad "ложный отказ на удалённой ветке: $out"; fi

# 2. Ветка существует и ВЛИТА — норма.
setup '| 1 | К1 | ЗАКРЫТО 2026-08-11 (окно p-merged7). |'
git -C "$TMP" branch p-merged7 2>/dev/null
out=$(bash "$G" "$TMP" 2>&1); rc=$?
if [ "$rc" -eq 0 ]; then ok "влитая ветка проходит"; else bad "ложный отказ на влитой ветке: $out"; fi

# 3. Ветка существует и НЕ влита — отказ. Это и есть случай 2026-08-11:
#    отчёт принят, класс записи переписан, ветка осталась в стороне.
setup '| 1 | К1 | ЗАКРЫТО 2026-08-11 (окно p-forgot9). |'
git -C "$TMP" checkout -q -b p-forgot9 2>/dev/null
printf 'x\n' > "$TMP/f.txt"
git -C "$TMP" add f.txt 2>/dev/null
git -C "$TMP" -c user.name=t -c user.email=t@t commit -q -m work 2>/dev/null
git -C "$TMP" checkout -q main 2>/dev/null
out=$(bash "$G" "$TMP" 2>&1); rc=$?
if [ "$rc" -eq 1 ] && echo "$out" | grep -q "p-forgot9"; then
    ok "ловит принятую, но не влитую ветку"
else
    bad "не поймал не влитую ветку (rc=$rc): $out"
fi

# 4. Строка БЕЗ признака приёмки ветку не требует: страж судит по тому, что
#    автор записи назвал сам, и не угадывает связи.
setup '| 1 | К1 | Статус: ОТКРЫТ. Работа идёт в p-inprogress3. |'
git -C "$TMP" checkout -q -b p-inprogress3 2>/dev/null
printf 'y\n' > "$TMP/g.txt"
git -C "$TMP" add g.txt 2>/dev/null
git -C "$TMP" -c user.name=t -c user.email=t@t commit -q -m wip 2>/dev/null
git -C "$TMP" checkout -q main 2>/dev/null
out=$(bash "$G" "$TMP" 2>&1); rc=$?
if [ "$rc" -eq 0 ]; then ok "открытая работа не требует слияния"; else bad "ложный отказ на открытой работе: $out"; fi

# 5. Страж назван на странице правил.
RULES="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)/docs/dev/rules-for-agents.md"
if grep -q "check-accepted-branch-merged.sh" "$RULES" 2>/dev/null; then
    ok "страж назван на странице правил"
else
    bad "страж не назван в docs/dev/rules-for-agents.md"
fi

# 6. МЕТКА [BRANCH-MENTION-OK] — ветка названа в ДИАГНОЗЕ, а не как носитель.
#    Повод — №994: строка ЗАКРЫТА стражем из `main`, а ветка в ней — ПРЕДМЕТ
#    ОШИБКИ (влит отставший origin-указатель). Страж — построчный греп,
#    а строка реестра занимает ОДНУ строку, то есть различить два случая без метки нельзя.
setup '| 1 | К1 | ЗАКРЫТО 2026-09-06 [BRANCH-MENTION-OK: p-diag5 — ветка названа как предмет ошибки]. |'
git -C "$TMP" checkout -q -b p-diag5 2>/dev/null
printf 'z\n' > "$TMP/h.txt"
git -C "$TMP" add h.txt 2>/dev/null
git -C "$TMP" -c user.name=t -c user.email=t@t commit -q -m diag 2>/dev/null
git -C "$TMP" checkout -q main 2>/dev/null
out=$(bash "$G" "$TMP" 2>&1); rc=$?
if [ "$rc" -eq 0 ]; then ok "метка снимает требование слияния"; else bad "метка не сработала: $out"; fi

# 7. МЕТКА ДЕЙСТВУЕТ ПОСТРОЧНО, а не на весь файл. Без этого одна метка
#    в одной строке глушила бы ту же ветку во ВСЁМ реестре — то есть выключала стража.
setup '| 1 | К1 | ЗАКРЫТО [BRANCH-MENTION-OK: p-diag5 — диагноз]. |
| 2 | К1 | ЗАКРЫТО 2026-09-06 (окно p-diag5). |'
git -C "$TMP" checkout -q -b p-diag5 2>/dev/null
printf 'z\n' > "$TMP/h.txt"
git -C "$TMP" add h.txt 2>/dev/null
git -C "$TMP" -c user.name=t -c user.email=t@t commit -q -m diag 2>/dev/null
git -C "$TMP" checkout -q main 2>/dev/null
out=$(bash "$G" "$TMP" 2>&1); rc=$?
if [ "$rc" -eq 1 ] && echo "$out" | grep -q "p-diag5"; then
    ok "метка действует только в своей строке"
else
    bad "метка из чужой строки погасила требование (rc=$rc): $out"
fi

# 8. ХЭШ-ПРЕДОК: работа в main, а ВЕТКА ЖИВА и ушла вперёд — это НОРМА.
#    Ровно случай №1143: прежняя мера краснела на самом факте того, что окно
#    продолжает работать. Живое окно — не долг.
setup 'PLACEHOLDER'
git -C "$TMP" checkout -q -b p-live274 2>/dev/null
printf 'a\n' > "$TMP/a.txt"
git -C "$TMP" add a.txt 2>/dev/null
git -C "$TMP" -c user.name=t -c user.email=t@t commit -q -m accepted 2>/dev/null
ACC=$(git -C "$TMP" rev-parse --short HEAD)
git -C "$TMP" checkout -q main 2>/dev/null
git -C "$TMP" merge -q --no-edit p-live274 2>/dev/null
git -C "$TMP" checkout -q p-live274 2>/dev/null
printf 'b\n' > "$TMP/b.txt"
git -C "$TMP" add b.txt 2>/dev/null
git -C "$TMP" -c user.name=t -c user.email=t@t commit -q -m 'work goes on' 2>/dev/null
git -C "$TMP" checkout -q main 2>/dev/null
printf '%s\n' "| 1 | K1 | ZAKRYTO (okno p-live274) [MERGED-AS: $ACC]. |" \
    > "$TMP/docs/plans/221.1-bug-sweep.md"
out=$(bash "$G" "$TMP" 2>&1); rc=$?
if [ "$rc" -eq 0 ]; then ok "хэш-предок: живая ветка впереди долгом не считается"; else bad "ложный отказ на живой ветке с хэшем: $out"; fi

# 9. ХЭШ НЕ ПРЕДОК main — ОТКАЗ. Вторая половина пробы: без неё случай 8
#    доказывал бы только то, что страж молчит.
setup 'PLACEHOLDER'
git -C "$TMP" checkout -q -b p-notin 2>/dev/null
printf 'c\n' > "$TMP/c.txt"
git -C "$TMP" add c.txt 2>/dev/null
git -C "$TMP" -c user.name=t -c user.email=t@t commit -q -m 'never merged' 2>/dev/null
LOST=$(git -C "$TMP" rev-parse --short HEAD)
git -C "$TMP" checkout -q main 2>/dev/null
printf '%s\n' "| 1 | K1 | ZAKRYTO (okno p-notin) [MERGED-AS: $LOST]. |" \
    > "$TMP/docs/plans/221.1-bug-sweep.md"
out=$(bash "$G" "$TMP" 2>&1); rc=$?
if [ "$rc" -eq 1 ] && echo "$out" | grep -q "$LOST"; then
    ok "хэш не предок main — отказ, и коммит назван"
else
    bad "не поймал хэш вне main (rc=$rc): $out"
fi

# 10. ХЭША НЕТ В ДЕРЕВЕ — тоже отказ, а не тихое прощение: запись ссылается
#     на то, чего не существует, и это хуже незакрытой ветки.
setup '| 1 | K1 | ZAKRYTO (okno p-ghost) [MERGED-AS: deadbee]. |'
out=$(bash "$G" "$TMP" 2>&1); rc=$?
if [ "$rc" -eq 1 ] && echo "$out" | grep -q "deadbee"; then
    ok "несуществующий коммит в приёмке — отказ"
else
    bad "несуществующий коммит прощён (rc=$rc): $out"
fi
if [ "$FAILED" -eq 0 ]; then echo "селфтест check-accepted-branch-merged: $CASES/$CASES ok"; exit 0; fi
echo "селфтест check-accepted-branch-merged: ЕСТЬ ПРОВАЛЫ" >&2
exit 1
