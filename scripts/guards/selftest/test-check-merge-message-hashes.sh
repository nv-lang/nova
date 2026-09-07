#!/usr/bin/env bash
# Селфтест scripts/guards/check-merge-message-hashes.sh.
#
# Проверяем ОБА направления. Первое очевидно — ловит ли чужой хеш. Второе
# важнее: не мешает ли работать. Страж, отказывающий всегда, будет обойдён в
# первый же день, и правило умрёт вместе с ним.
#
# ФОРМЫ ФИКСТУР ВЗЯТЫ ИЗ ДЕРЕВА, а не придуманы: заголовок вида
# «merge p274-novac (<хеш>): …» — это ровно то, чем интегратор писал слияния
# 2026-09-06, и ровно на такой строке дефект №994 и случился. Зелёная проба на
# форме, которой в дереве нет, не доказывает ничего (реестр №989).
#
# Работаем во ВРЕМЕННОМ репозитории; `git config` пользователя не трогаем —
# авторство задаётся флагами `-c` на конкретный вызов.
set -u
export LC_ALL=C

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
G="$ROOT/scripts/guards/check-merge-message-hashes.sh"
FAILED=0
# Число случаев в итоге печатает СЧЁТЧИК, а не рука (check-selftest-honest-count).
CASES=0
ok()  { CASES=$((CASES+1)); echo "  ok: $1"; }
bad() { CASES=$((CASES+1)); echo "  ПРОВАЛ: $1" >&2; FAILED=1; }

TMP=$(mktemp -d); trap 'rm -rf "$TMP"' EXIT
GC="git -C $TMP -c user.name=selftest -c user.email=selftest@example.com -c commit.gpgsign=false"

git init -q -b main "$TMP" 2>/dev/null || { echo "нет врем. репозитория" >&2; exit 1; }
echo base > "$TMP/f.txt"; $GC add f.txt >/dev/null 2>&1; $GC commit -q -m base >/dev/null 2>&1
BASE_SHA=$($GC rev-parse HEAD)
$GC branch base-mark >/dev/null 2>&1

# Ветка со своим коммитом — её и будем сливать.
$GC checkout -q -b side main 2>/dev/null
echo side > "$TMP/s.txt"; $GC add s.txt >/dev/null 2>&1; $GC commit -q -m "side work" >/dev/null 2>&1
SIDE_SHA=$($GC rev-parse HEAD)
# Второй, НЕ вливаемый коммит — источник «чужого» хеша.
$GC checkout -q -b other main 2>/dev/null
echo other > "$TMP/o.txt"; $GC add o.txt >/dev/null 2>&1; $GC commit -q -m "other work" >/dev/null 2>&1
OTHER_SHA=$($GC rev-parse HEAD)
$GC checkout -q main 2>/dev/null

run() { NOVA_MERGE_MSG_ALLOW="${2:-$TMP/none}" bash "$G" "$TMP" "base-mark" >"$TMP/out" 2>"$TMP/err"; }

# --- 1. слияний в диапазоне нет — зелёный, и он ГОВОРИТ об этом --------------
if run && grep -q "слияний в диапазоне" "$TMP/out"; then
    ok "нет слияний — зелёный и назвал причину"
else
    bad "молчание вместо строки о пустом диапазоне: $(cat "$TMP/out" "$TMP/err")"
fi

# --- 2. ГЛАВНЫЙ случай: сообщение называет НЕВЛИТЫЙ коммит -------------------
$GC merge --no-ff -m "merge side ($OTHER_SHA): work that is not here" side >/dev/null 2>&1
if run; then
    bad "чужой хеш в сообщении слияния прошёл зелёным"
else
    grep -q "НЕДОСТИЖИМЫЙ" "$TMP/err" && ok "чужой хеш пойман и назван" || bad "красный, но без причины: $(cat "$TMP/err")"
fi

# --- 3. тот же коммит в списке исключений — зелёный, и пропуск НАЗВАН --------
MERGE_SHA=$($GC rev-parse HEAD)
printf '%s — selftest\n' "$MERGE_SHA" > "$TMP/allow"
if run "" "$TMP/allow" && grep -q "в списке исключений" "$TMP/out"; then
    ok "исключение пропускает и называет себя"
else
    bad "исключение не сработало: $(cat "$TMP/out" "$TMP/err")"
fi

# --- 4. сообщение называет ВЛИТЫЙ коммит — зелёный ---------------------------
#     Направление «не мешает работать»: без него страж запрещал бы называть
#     хеш в сообщении вообще, и его отключат.
$GC reset -q --hard main~1 2>/dev/null
$GC merge --no-ff -m "merge side ($SIDE_SHA): the commit actually merged" side >/dev/null 2>&1
if run && grep -q "все достижимы" "$TMP/out"; then
    ok "влитый хеш пропускается"
else
    bad "ложный отказ на влитом хеше: $(cat "$TMP/out" "$TMP/err")"
fi

# --- 5. хеша нет вовсе — отдельная диагностика, не «недостижим» --------------
$GC reset -q --hard main~1 2>/dev/null
$GC merge --no-ff -m "merge side (deadbeefdeadbeef): no such object" side >/dev/null 2>&1
if run; then
    bad "несуществующий хеш прошёл зелёным"
else
    grep -q "такого коммита нет вовсе" "$TMP/err" && ok "несуществующий хеш назван отдельно" || bad "спутан с недостижимым: $(cat "$TMP/err")"
fi

# --- 6. форма `репозиторий@хеш` — чужая репа, не судится ---------------------
#     Конвенция требует эту форму именно для того, чтобы было видно: хеш не наш.
$GC reset -q --hard main~1 2>/dev/null
$GC merge --no-ff -m "merge side: pulls nova-tls@7a44776a1b2c3d4 for the fix" side >/dev/null 2>&1
if run && grep -q "все достижимы" "$TMP/out"; then
    ok "форма репозиторий@хеш не судится"
else
    bad "чужая репа принята за наш хеш: $(cat "$TMP/out" "$TMP/err")"
fi

# --- 7. базы нет — зелёный, но МОЛЧАНИЯ быть не должно ----------------------
out=$(NOVA_MERGE_MSG_ALLOW="$TMP/none" bash "$G" "$TMP" "no-such-ref" 2>&1); rc=$?
if [ "$rc" -eq 0 ] && echo "$out" | grep -q "судить нечего"; then
    ok "нет базы — сказано вслух, а не пропущено молча"
else
    bad "отсутствие базы обработано молча (код $rc): $out"
fi

# --- 8. tree от `git merge-tree` в сообщении — НЕ коммит и НЕ нарушение ------
#     Интегратор пишет в сообщение id дерева, которым проверял слияние ДО
#     коммита (2026-09-06: a57208585 назвал e2350bdbf). Страж объявлял его
#     «коммитом, которого нет вовсе» — ложный текст отказа на честной проверке.
$GC reset -q --hard main~1 2>/dev/null
TREE=$($GC merge-tree HEAD side 2>/dev/null | head -1)
$GC merge --no-ff -m "merge side: inspected via merge-tree $TREE beforehand" side >/dev/null 2>&1
if [ -n "$TREE" ] && run && grep -q "не коммит" "$TMP/out"; then
    ok "tree из merge-tree опознан как не-коммит, слияние не отвергнуто"
else
    bad "tree принят за несуществующий коммит (tree='$TREE'): $(cat "$TMP/out" "$TMP/err")"
fi

if [ "$FAILED" -eq 0 ]; then echo "селфтест check-merge-message-hashes: $CASES/$CASES ok"; exit 0; fi
echo "селфтест check-merge-message-hashes: ЕСТЬ ПРОВАЛЫ" >&2
exit 1
