#!/usr/bin/env bash
# scripts/guards/selftest/test-sync-mirrors.sh — догон зеркал вынесен из-под замка
# вливания, и при этом зеркала по-прежнему получают только ЗЕЛЁНЫЙ main.
#
# ЗАЧЕМ (задача #51, 2026-10-08): `land-task.sh` пушил зеркала под замком `crew_task
# merge` и ждал CI на новом main ~15–20 мин. Теперь он печатает LANDED сразу после пуша
# origin/main, а зеркала догоняет `scripts/tools/sync-mirrors.sh`. Правка такого рода
# опасна в обе стороны: догон мог бы запушить красный main (ослабление) или старый хеш
# задачи, когда main уже ушёл дальше (догон прошлого). Самотест держит обе.
#
# ЧТО ПРОВЕРЯЕТСЯ (настоящие репозитории: origin и два зеркала — голые каталоги;
# подменены только `gh` — список прогонов из файла — и `python` — вердикт стража
# check-push-proven-by-ci по списку зелёных хешей):
#   A. main ушёл на два коммита, оба зелёные -> зеркала на ВЕРШИНЕ, не на промежуточном;
#   B. повтор -> rc 0, зеркала не тронуты (идемпотентность);
#   C. вершина КРАСНАЯ, предок зелёный -> rc 4, красная не запушена, зеркала на зелёном;
#   D. зелёного новее зеркал нет вовсе -> rc 4, зеркала не тронуты;
#   E. CI вершины ещё идёт, время вышло -> rc 7, зеркала на последнем зелёном;
#   F. --dry-run при отстающих зеркалах -> rc 3, последняя строка MIRRORS-DRY would-push=…,
#      ни одного пуша (сухой прогон «догнано» не говорит — приёмка #51, круг 1);
#   F2. --dry-run при зеркалах на вершине -> rc 0, MIRRORS-SYNCED;
#   F3. --dry-run, CI вершины идёт -> rc 3, would-push=none;
#   G. зеркало отказывает пушу при завершённом CI -> rc 6;
#   G2. отказ, потому что CI цели снова пошёл (гонка после LANDED) -> ждёт и догоняет, rc 0;
#   H. land-task.sh: LANDED после пуша origin/main, зеркала НЕ тронуты, integrate/t<N> снята;
#   R. страж требует ВОСЬМОЙ workflow -> семи завершённых мало, rc 7; восьмой пришёл -> rc 0
#      (список REQUIRED читается из стража, не копией);
#   I. грязное дерево -> rc 2 до пуша, а не ложный rc 6 от pre-push.
# Названия обязательных workflow самотест берёт из НАСТОЯЩЕГО стража тем же разбором.
set -u
export LC_ALL=C
NAME="test-sync-mirrors"
ROOT="${1:-$(cd "$(dirname "$0")/../../.." && pwd)}"
SYNC="$ROOT/scripts/tools/sync-mirrors.sh"
LAND="$ROOT/scripts/tools/land-task.sh"
REAL_PROOF="$ROOT/scripts/guards/check-push-proven-by-ci.py"
for f in "$SYNC" "$LAND" "$REAL_PROOF"; do [ -f "$f" ] || { echo "$NAME: FAIL — нет $f" >&2; exit 1; }; done
REQ_BLOCK=$(sed -n '/^REQUIRED = \[/,/\]/p' "$REAL_PROOF")
REQ_NAMES=$(printf '%s\n' "$REQ_BLOCK" | grep -o '"[^"]*"' | tr -d '"')
[ -n "$REQ_NAMES" ] || { echo "$NAME: FAIL — в $REAL_PROOF не разобран REQUIRED = [...]" >&2; exit 1; }

TMP=$(mktemp -d 2>/dev/null || mktemp -d -t syncm)
trap 'rm -rf "$TMP"' EXIT
fails=0
CASES=0

note() { echo "$NAME: $*"; }
bad()  { echo "$NAME: FAIL — $*" >&2; fails=$((fails + 1)); }
git_q() { git -C "$1" -c user.name=T -c user.email=t@t -c commit.gpgsign=false "${@:2}" >/dev/null 2>&1; }

# ── подмены gh и python ──────────────────────────────────────────────────────
BIN="$TMP/bin"; mkdir -p "$BIN"
RUNS="$TMP/runs.txt"; GREEN="$TMP/green.txt"; : > "$RUNS"; : > "$GREEN"
cat > "$BIN/gh" <<EOF
#!/usr/bin/env bash
# Скрипты зовут gh run list с --jq; подмена отдаёт уже готовые строки
# "<sha> <workflow> <status> <conclusion>".
cat "$RUNS"
# Гонка G2: зеркало отказало, дописав незавершённый прогон; его видит ровно один
# следующий вызов, затем прогон «завершается» (строка уходит).
if [ -f "$TMP/raced" ] && [ ! -f "$TMP/race-shown" ]; then
    : > "$TMP/race-shown"; grep -v ' in_progress ' "$RUNS" > "$RUNS.x"; mv "$RUNS.x" "$RUNS"
fi
EOF
cat > "$BIN/python" <<EOF
#!/usr/bin/env bash
case "\${1:-}" in
    *check-push-proven-by-ci.py)
        if grep -qx "\${2:-}" "$GREEN"; then echo "fake-proof: ok \${2:0:9}"; exit 0; fi
        echo "fake-proof: \${2:0:9} not proven" >&2; exit 1 ;;
esac
echo "fake python: unexpected call \$*" >&2; exit 99
EOF
chmod +x "$BIN/gh" "$BIN/python"
export PATH="$BIN:$PATH"
unset CREW_ROLE CREW_REVIEW_N CREW_SESSION_ID OPENCODE_SESSION_ID

# Прогоны всех обязательных workflow хеша в состоянии st (completed|in_progress).
runs() { local sha=$1 st=$2 c=success w
    [ "$st" = completed ] || c=
    for w in $REQ_NAMES; do
        echo "$sha $w $st $c" >> "$RUNS"
    done; }
green() { echo "$1" >> "$GREEN"; }
reset_ci() { : > "$RUNS"; : > "$GREEN"; }

# ── репозитории ──────────────────────────────────────────────────────────────
for r in origin gitverse sourcecraft; do git init --bare -q "$TMP/$r.git" 2>/dev/null; done
W="$TMP/work"; git init -q "$W" 2>/dev/null
for r in origin gitverse sourcecraft; do git -C "$W" remote add "$r" "$TMP/$r.git"; done
mkdir -p "$W/scripts/guards"
{ echo "# stand-in: the selftest replaces python; REQUIRED copied from the real guard"
  printf '%s\n' "$REQ_BLOCK"; } > "$W/scripts/guards/check-push-proven-by-ci.py"
git_q "$W" add scripts/guards/check-push-proven-by-ci.py
git_q "$W" commit -m c1
git_q "$W" branch -M main
C1=$(git -C "$W" rev-parse HEAD)
for r in origin gitverse sourcecraft; do git_q "$W" push "$r" main; done
commit() { echo "$1" > "$W/f.txt"; git_q "$W" add f.txt; git_q "$W" commit -m "$1"; git -C "$W" rev-parse HEAD; }
C2=$(commit c2); C3=$(commit c3)
git_q "$W" push origin main

at()  { git -C "$W" ls-remote "$TMP/$1.git" refs/heads/main | cut -f1; }
set_ref() { git -C "$TMP/$1.git" update-ref refs/heads/main "$2"; }
mirrors_to() { set_ref gitverse "$1"; set_ref sourcecraft "$1"; }
sync() { (cd "$W" && SYNC_MIRRORS_TIMEOUT=${TO:-600} SYNC_MIRRORS_POLL=1 bash "$SYNC" "$@") > "$TMP/out.txt" 2>&1; echo $?; }
# Случай: имя, ожидаемый код, фактический, ожидаемое положение обоих зеркал.
check() { local name=$1 want_rc=$2 rc=$3 want_at=$4 gv sc
    CASES=$((CASES + 1)); gv=$(at gitverse); sc=$(at sourcecraft)
    if [ "$rc" = "$want_rc" ] && [ "$gv" = "$want_at" ] && [ "$sc" = "$want_at" ]; then
        note "$name ok (rc=$rc, зеркала ${gv:0:9})"
    else
        bad "$name: rc=$rc (ждали $want_rc), gitverse ${gv:0:9} sourcecraft ${sc:0:9} (ждали ${want_at:0:9})"
        sed 's/^/    | /' "$TMP/out.txt" >&2
    fi; }

# A. main ушёл вперёд на два зелёных коммита: пушится вершина, а не промежуточный.
reset_ci; runs "$C2" completed; green "$C2"; runs "$C3" completed; green "$C3"
check "A вершина при сдвинутом main" 0 "$(sync)" "$C3"
# Конечное положение то же и у скрипта, который шагает от старейшего зелёного (цикл
# доходит до вершины); различает их число пушей — один на зеркало, сразу на вершину.
CASES=$((CASES + 1)); np=$(grep -c 'sync-mirrors: .* gitverse: ' "$TMP/out.txt" || true)
if [ "$np" = 1 ] && grep -q "gitverse: ${C1:0:9} -> ${C3:0:9}" "$TMP/out.txt"; then
    note "A2 один пуш, сразу ${C1:0:9} -> ${C3:0:9}"
else
    bad "A2: пушей в gitverse $np, ждали один ${C1:0:9} -> ${C3:0:9} — промежуточный хеш пушился"
    sed 's/^/    | /' "$TMP/out.txt" >&2
fi

# B. Повтор — ничего не делает.
check "B идемпотентность" 0 "$(sync)" "$C3"

# C. Вершина красная, предок зелёный: красная не уходит, зеркала на зелёном, rc 4.
C4=$(commit c4); git_q "$W" push origin main
mirrors_to "$C1"
reset_ci; runs "$C2" completed; green "$C2"; runs "$C3" completed; green "$C3"; runs "$C4" completed
check "C красная вершина" 4 "$(sync)" "$C3"
grep -q 'MIRRORS-FAIL rc=4' "$TMP/out.txt" || bad "C: последняя строка не MIRRORS-FAIL rc=4"

# D. Зелёного новее зеркал нет: зеркала не тронуты.
mirrors_to "$C1"
reset_ci; runs "$C4" completed
check "D нет зелёного" 4 "$(sync)" "$C1"

# E. CI вершины идёт, время вышло: rc 7, зеркала подтянуты до последнего зелёного.
mirrors_to "$C1"
reset_ci; runs "$C3" completed; green "$C3"; runs "$C4" in_progress
TO=0; check "E CI не завершился" 7 "$(sync)" "$C3"; TO=

# Последняя строка вывода прошлого прогона — то, что читает человек и /save.
last_is() { local name=$1 want=$2 got
    CASES=$((CASES + 1)); got=$(tail -n 1 "$TMP/out.txt")
    if [ "$got" = "$want" ]; then note "$name ok: последняя строка '$got'"
    else bad "$name: последняя строка '$got', ждали '$want'"; fi; }

# F. Пробный прогон при отстающих зеркалах ничего не пушит и «догнано» НЕ говорит.
mirrors_to "$C1"
reset_ci; runs "$C4" completed; green "$C4"
check "F --dry-run, зеркала позади" 3 "$(sync --dry-run)" "$C1"
last_is "F2 строка сухого прогона" "MIRRORS-DRY would-push=${C4:0:9} main=${C4:0:9}"

# F3. Пробный прогон при зеркалах на вершине — вот тогда MIRRORS-SYNCED. Зеркала коммита
# C4 ещё не видели: туда — пушем, не update-ref.
for r in gitverse sourcecraft; do git_q "$W" push "$r" "$C4:refs/heads/main"; done
check "F3 --dry-run, зеркала на вершине" 0 "$(sync --dry-run)" "$C4"
last_is "F3 строка" "MIRRORS-SYNCED main=${C4:0:9}"

# F4. Пробный прогон, CI вершины идёт, зелёного новее зеркал нет: would-push=none.
mirrors_to "$C3"
reset_ci; runs "$C3" completed; green "$C3"; runs "$C4" in_progress
check "F4 --dry-run, CI идёт" 3 "$(sync --dry-run)" "$C3"
last_is "F4 строка" "MIRRORS-DRY would-push=none main=${C4:0:9}"
reset_ci; runs "$C4" completed; green "$C4"

# G. Зеркало отказывает: rc 6.
mirrors_to "$C1"
printf '#!/bin/sh\nexit 1\n' > "$TMP/gitverse.git/hooks/pre-receive"; chmod +x "$TMP/gitverse.git/hooks/pre-receive"
rc=$(sync); CASES=$((CASES + 1))
if [ "$rc" = 6 ] && [ "$(at gitverse)" = "$C1" ]; then note "G отказ зеркала ok (rc=6)"
else bad "G: rc=$rc (ждали 6), gitverse ${C1:0:9}->$(at gitverse | cut -c1-9)"; sed 's/^/    | /' "$TMP/out.txt" >&2; fi
rm -f "$TMP/gitverse.git/hooks/pre-receive"

# G2. Гонка сразу после LANDED: проверка видела CI цели завершённым, а к пушу на том же
# хеше зарегистрировались прогоны main, и pre-push отказал. Это ожидание, не rc 6.
mirrors_to "$C1"
cat > "$TMP/gitverse.git/hooks/pre-receive" <<EOF
#!/bin/sh
[ -f "$TMP/raced" ] && exit 0
: > "$TMP/raced"; echo "$C4 nova-gate in_progress " >> "$RUNS"
echo "run still queued -- wait" >&2; exit 1
EOF
chmod +x "$TMP/gitverse.git/hooks/pre-receive"
check "G2 отказ при снова идущем CI — ждёт и догоняет" 0 "$(sync)" "$C4"
grep -q 'отказ при снова идущем CI' "$TMP/out.txt" || bad "G2: гонка не случилась — случай ничего не проверил"
rm -f "$TMP/gitverse.git/hooks/pre-receive" "$TMP/raced" "$TMP/race-shown"

# H. land-task: LANDED сразу после пуша origin/main, зеркала не трогает.
mirrors_to "$C4"; set_ref origin "$C4"
git_q "$W" checkout -b t51
C5=$(commit c5)
git_q "$W" checkout main
reset_ci; runs "$C5" completed; green "$C5"
(cd "$W" && LAND_CI_TIMEOUT=60 bash "$LAND" 51 "$C5") > "$TMP/out.txt" 2>&1; rc=$?
CASES=$((CASES + 1))
last=$(tail -n 1 "$TMP/out.txt")
cand=$(git -C "$W" ls-remote "$TMP/origin.git" refs/heads/integrate/t51 | cut -f1)
if [ "$rc" = 0 ] && [ "$last" = "LANDED task=#51 main=${C5:0:9}" ] && [ "$(at origin)" = "$C5" ] \
   && [ "$(at gitverse)" = "$C4" ] && [ "$(at sourcecraft)" = "$C4" ] && [ -z "$cand" ]; then
    note "H land-task ok: LANDED, origin/main ${C5:0:9}, зеркала не тронуты (${C4:0:9}), integrate/t51 снята"
else
    bad "H land-task: rc=$rc, последняя строка '$last', origin $(at origin | cut -c1-9), gitverse $(at gitverse | cut -c1-9), sourcecraft $(at sourcecraft | cut -c1-9), integrate/t51 '${cand:0:9}'"
    sed 's/^/    | /' "$TMP/out.txt" >&2
fi

# H2. За ним sync-mirrors догоняет зеркала до той же вершины.
check "H2 догон после LANDED" 0 "$(sync)" "$C5"

# R. Страж требует ещё один workflow: число берётся из стража, семи завершённых мало.
# Правка — локальным коммитом (дерево чистое, origin/main не меняется).
sed -i 's/^\]$/    "extra-wf",\n]/' "$W/scripts/guards/check-push-proven-by-ci.py"
git_q "$W" commit -am "guard: one more required workflow"
mirrors_to "$C4"
reset_ci; runs "$C5" completed; green "$C5"
TO=0; check "R восьмой workflow ещё не прогнан" 7 "$(sync)" "$C4"; TO=
echo "$C5 extra-wf completed success" >> "$RUNS"
check "R2 восьмой пришёл" 0 "$(sync)" "$C5"

# I. Грязное дерево: отказ ДО пуша, rc 2, а не ложный rc 6 от pre-push.
mirrors_to "$C4"
echo dirty >> "$W/f.txt"
check "I грязное дерево" 2 "$(sync)" "$C4"
grep -q 'грязное' "$TMP/out.txt" || bad "I: причина не названа"
git -C "$W" checkout -q -- f.txt

if [ "$fails" -eq 0 ]; then
    echo "$NAME ok: $CASES/$CASES — зеркала получают только зелёный main и только новейший, land-task их не ждёт"
    exit 0
fi
echo "$NAME: FAIL — провалов: $fails из $CASES" >&2
exit 1
