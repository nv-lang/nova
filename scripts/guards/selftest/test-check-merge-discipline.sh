#!/usr/bin/env bash
# Селфтест scripts/guards/check-merge-discipline.sh.
#
# Проверяем ОБА направления. Первое очевидно — ловит ли отказ. Второе важнее:
# не мешает ли работать. Страж, который отказывает всегда, будет обойдён в
# первый же день, и правило умрёт вместе с ним.
#
# Работаем во ВРЕМЕННОМ репозитории; `git config` пользователя не трогаем —
# авторство задаётся флагами `-c` на конкретный вызов (общий .git репозитория
# Nova делится между worktree).
set -u
export LC_ALL=C

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
G="$ROOT/scripts/guards/check-merge-discipline.sh"
FAILED=0
# Число случаев в итоге печатает СЧЁТЧИК, а не рука: рукописное расходится с телом
# на первом же добавленном случае и не краснеет никогда (check-selftest-honest-count).
CASES=0
ok()  { CASES=$((CASES+1)); echo "  ok: $1"; }
bad() { CASES=$((CASES+1)); echo "  ПРОВАЛ: $1" >&2; FAILED=1; }

TMP=$(mktemp -d); trap 'rm -rf "$TMP"' EXIT
GC="git -C $TMP -c user.name=selftest -c user.email=selftest@example.com -c commit.gpgsign=false"

git init -q -b main "$TMP" 2>/dev/null || { echo "нет врем. репозитория" >&2; exit 1; }
echo base > "$TMP/f.txt"; $GC add f.txt >/dev/null 2>&1; $GC commit -q -m base >/dev/null 2>&1

V="$TMP/verdict"

# 1. Вердикта нет — отказ.
rm -f "$V"
out=$(NOVA_GATE_VERDICT="$V" bash "$G" "$TMP" 2>&1); rc=$?
if [ "$rc" -eq 1 ] && echo "$out" | grep -q 'вердикта гейта нет'; then
    ok "отказ, когда гейта не было"
else
    bad "не отказал без вердикта (код $rc): $out"
fi

# 2. Вердикт красный — отказ.
echo "RC=1 SEC=380" > "$V"
out=$(NOVA_GATE_VERDICT="$V" bash "$G" "$TMP" 2>&1); rc=$?
if [ "$rc" -eq 1 ] && echo "$out" | grep -q 'КРАСНЫЙ'; then
    ok "отказ на красном гейте"
else
    bad "не отказал на красном (код $rc): $out"
fi

# 3. Вердикт зелёный, свежий и НАЗЫВАЮЩИЙ СВОЙ ЯРУС — пропуск. Это направление
#    важнее первых двух: страж, отказывающий всегда, будет обойдён и правило
#    умрёт. УРОВЕНЬ (`:push`) ОБЯЗАТЕЛЕН с №995: `loop` не судит корпус вовсе,
#    а выглядел так же, как push. HASH здесь намеренно НЕ указан — вердикт без хеша законен (старый
#    формат ещё может лежать на машине), проверяется он только когда назван.
echo "RC=0 SEC=2412 TIER=main:push" > "$V"
out=$(NOVA_GATE_VERDICT="$V" bash "$G" "$TMP" 2>&1); rc=$?
if [ "$rc" -eq 0 ] && echo "$out" | grep -q 'слияние законно'; then
    ok "пропускает при зелёном и свежем гейте"
else
    bad "ложный отказ при зелёном гейте (код $rc): $out"
fi

# 4. Вердикт зелёный, но СТАРШЕ HEAD — отказ. Без этой проверки один зелёный
#    гейт недельной давности разрешал бы слияния вечно (класс №473:
#    «проверка есть, но ничего не проверяет»).
echo "RC=0 SEC=2412" > "$V"
touch -d '2020-01-01' "$V" 2>/dev/null || touch -t 202001010000 "$V"
out=$(NOVA_GATE_VERDICT="$V" bash "$G" "$TMP" 2>&1); rc=$?
if [ "$rc" -eq 1 ] && echo "$out" | grep -q 'СТАРШЕ HEAD'; then
    ok "отказ на устаревшем вердикте"
else
    bad "устаревший вердикт принят (код $rc): $out"
fi

# 5. Осознанный обход работает (вливается сам фикс красноты).
echo "RC=1 SEC=380" > "$V"
out=$(NOVA_GATE_VERDICT="$V" NOVA_MERGE_ALLOW_RED=1 bash "$G" "$TMP" 2>&1); rc=$?
if [ "$rc" -eq 0 ] && echo "$out" | grep -q 'ОБХОД'; then
    ok "осознанный обход пропускает и называет себя"
else
    bad "обход не работает (код $rc): $out"
fi

# 6. На НЕ главной ветке правило не применяется — окна должны работать свободно.
$GC checkout -q -b feature 2>/dev/null
rm -f "$V"
out=$(NOVA_GATE_VERDICT="$V" bash "$G" "$TMP" 2>&1); rc=$?
if [ "$rc" -eq 0 ] && echo "$out" | grep -q 'пропуск'; then
    ok "не мешает работе в ветке окна"
else
    bad "правило сработало вне главной ветки (код $rc): $out"
fi

$GC checkout -q main 2>/dev/null   # случай 6 уводит с main — возвращаемся ЯВНО

# ── №988: вердикт обязан называть ярус, а ярус novac — судить свои пути ────
# Проба, записанная в приёмке строки №988 дословно: «вердикт яруса main
# подсовывается на слияние novac-ветки — страж ОБЯЗАН покраснеть».

# 7. Вердикт БЕЗ яруса — отказ. До 2026-09-06 это был единственный формат, и
#    именно поэтому слияние novac-ветки проходило по вердикту, который её файлов
#    не видел.
echo "RC=0 SEC=2412" > "$V"
out=$(NOVA_GATE_VERDICT="$V" bash "$G" "$TMP" 2>&1); rc=$?
if [ "$rc" -eq 1 ] && echo "$out" | grep -q 'не называет ЯРУС'; then
    ok "отказ на безымянном вердикте (старый формат)"
else
    bad "безымянный вердикт принят (код $rc): $out"
fi

# 8. Вердикт ЧУЖОГО яруса подан как основной — отказ. Ярусы судят разное и не
#    заменяют друг друга.
echo "RC=0 SEC=900 TIER=novac:push" > "$V"
out=$(NOVA_GATE_VERDICT="$V" bash "$G" "$TMP" 2>&1); rc=$?
if [ "$rc" -eq 1 ] && echo "$out" | grep -q "яруса 'novac' подан как основной"; then
    ok "отказ, когда вердикт novac подсунут вместо основного"
else
    bad "чужой ярус принят за основной (код $rc): $out"
fi

# 9. Хеш назван и НЕ совпадает с HEAD — отказ. Свежесть по времени этого не
#    ловит: гейт мог идти на другой ветке в ту же минуту.
echo "RC=0 SEC=2412 TIER=main:push HASH=deadbeef" > "$V"
out=$(NOVA_GATE_VERDICT="$V" bash "$G" "$TMP" 2>&1); rc=$?
if [ "$rc" -eq 1 ] && echo "$out" | grep -q 'ДРУГОЕ дерево'; then
    ok "отказ, когда вердикт судил другое дерево"
else
    bad "вердикт чужого дерева принят (код $rc): $out"
fi

# ── Слияние в ходу: MERGE_HEAD есть, и по нему видно, ЧТО оно приносит ──────
$GC checkout -q -b nvbr main 2>/dev/null
mkdir -p "$TMP/novac/src"
echo 'fn main() {}' > "$TMP/novac/src/a.rs"
$GC add novac/src/a.rs >/dev/null 2>&1; $GC commit -q -m "novac change" >/dev/null 2>&1
$GC checkout -q main 2>/dev/null
$GC merge --no-commit --no-ff nvbr >/dev/null 2>&1
HEAD_SHA=$(git -C "$TMP" rev-parse HEAD 2>/dev/null)
# Вердикт яруса novac относится к ВХОДЯЩЕЙ ветке — этот ярус гоняется у себя
# окном 274, а не на моём дереве, — значит сверяться он обязан с MERGE_HEAD.
MERGE_SHA=$(git -C "$TMP" rev-parse MERGE_HEAD 2>/dev/null)
NV="$TMP/novac-verdict"

if ! git -C "$TMP" rev-parse -q --verify MERGE_HEAD >/dev/null 2>&1; then
    bad "не удалось создать состояние слияния (MERGE_HEAD нет) — случаи 10-12 не проверены"
else
    # 10. Слияние приносит novac-пути, вердикта яруса novac нет — ОТКАЗ.
    #     Это и есть дыра №988 в чистом виде.
    echo "RC=0 SEC=2412 TIER=main:push HASH=$HEAD_SHA" > "$V"
    rm -f "$NV"
    out=$(NOVA_GATE_VERDICT="$V" NOVA_NOVAC_VERDICT="$NV" bash "$G" "$TMP" 2>&1); rc=$?
    if [ "$rc" -eq 1 ] && echo "$out" | grep -q 'яруса novac'; then
        ok "отказ: слияние несёт novac-пути, а вердикта его яруса нет"
    else
        bad "novac-слияние прошло по одному основному вердикту (код $rc): $out"
    fi

    # 11. Тот же случай, но вердикт яруса novac есть, зелёный и свежий —
    #     ПРОПУСК. Направление «не мешает работать» обязательно: страж,
    #     запрещающий novac-слияния вовсе, будет обойдён в первый же день.
    echo "RC=0 SEC=900 TIER=novac HASH=$MERGE_SHA" > "$NV"
    out=$(NOVA_GATE_VERDICT="$V" NOVA_NOVAC_VERDICT="$NV" bash "$G" "$TMP" 2>&1); rc=$?
    if [ "$rc" -eq 0 ] && echo "$out" | grep -q 'слияние законно'; then
        ok "пропускает novac-слияние, когда есть вердикты ОБОИХ ярусов"
    else
        bad "ложный отказ при двух зелёных вердиктах (код $rc): $out"
    fi

    # 12. Вердикт яруса novac КРАСНЫЙ — отказ.
    echo "RC=1 SEC=900 TIER=novac HASH=$MERGE_SHA" > "$NV"
    out=$(NOVA_GATE_VERDICT="$V" NOVA_NOVAC_VERDICT="$NV" bash "$G" "$TMP" 2>&1); rc=$?
    if [ "$rc" -eq 1 ] && echo "$out" | grep -q 'novac КРАСНЫЙ'; then
        ok "отказ на красном гейте novac"
    else
        bad "красный novac пропущен (код $rc): $out"
    fi

    # 12б. Вердикт яруса novac зелёный, но судил ДРУГОЕ содержимое — отказ.
    #      Свежесть по времени этого не ловит вовсе: файл только что создан.
    echo "RC=0 SEC=900 TIER=novac HASH=0123456789abcdef0123456789abcdef01234567" > "$NV"
    out=$(NOVA_GATE_VERDICT="$V" NOVA_NOVAC_VERDICT="$NV" bash "$G" "$TMP" 2>&1); rc=$?
    if [ "$rc" -eq 1 ] && echo "$out" | grep -q 'ДРУГОЕ содержимое'; then
        ok "отказ: вердикт novac о другом содержимом"
    else
        bad "вердикт novac о чужом дереве принят (код $rc): $out"
    fi

    # 12в. Выборка со швом (`novac-sample`) не заменяет полный ярус.
    echo "RC=0 SEC=90 TIER=novac-sample HASH=$MERGE_SHA" > "$NV"
    out=$(NOVA_GATE_VERDICT="$V" NOVA_NOVAC_VERDICT="$NV" bash "$G" "$TMP" 2>&1); rc=$?
    if [ "$rc" -eq 1 ] && echo "$out" | grep -q "novac-sample"; then
        ok "отказ: выборка со швом не заменяет полный ярус"
    else
        bad "выборка принята за полный ярус (код $rc): $out"
    fi

    $GC merge --abort >/dev/null 2>&1
fi

# 13. Слияние БЕЗ novac-путей проходит по одному основному вердикту — ложного
#     отказа быть не должно, иначе правило умрёт от неудобства.
$GC checkout -q -b plainbr main 2>/dev/null
echo change > "$TMP/f.txt"
$GC add f.txt >/dev/null 2>&1; $GC commit -q -m "plain change" >/dev/null 2>&1
$GC checkout -q main 2>/dev/null
$GC merge --no-commit --no-ff plainbr >/dev/null 2>&1
HEAD_SHA=$(git -C "$TMP" rev-parse HEAD 2>/dev/null)
echo "RC=0 SEC=2412 TIER=main:push HASH=$HEAD_SHA" > "$V"
rm -f "$NV"
out=$(NOVA_GATE_VERDICT="$V" NOVA_NOVAC_VERDICT="$NV" bash "$G" "$TMP" 2>&1); rc=$?
if [ "$rc" -eq 0 ] && echo "$out" | grep -q 'слияние законно'; then
    ok "обычное слияние не требует вердикта яруса novac"
else
    bad "ложный отказ на слиянии без novac-путей (код $rc): $out"
fi
$GC merge --abort >/dev/null 2>&1
$GC checkout -q main 2>/dev/null

# --- №995: вердикт обязан называть УРОВЕНЬ, а не только ярус -----------
$GC merge --abort >/dev/null 2>&1
$GC checkout -q main 2>/dev/null

# 16. Ярус назван, УРОВЕНЬ НЕТ — отказ. До №995 это был единственный
#     формат, и вердикт текстового яруса loop открывал слияние так же, как push.
echo "RC=0 SEC=2412 TIER=main" > "$V"
out=$(NOVA_GATE_VERDICT="$V" bash "$G" "$TMP" 2>&1); rc=$?
if [ "$rc" -eq 1 ] && echo "$out" | grep -q "не называет УРОВЕНЬ"; then
    ok "отказ на вердикте без уровня"
else
    bad "вердикт без уровня принят (код $rc): $out"
fi

# 17. Уровень `loop` — отказ, и причина названа СОДЕРЖАТЕЛЬНО.
echo "RC=0 SEC=200 TIER=main:loop" > "$V"
out=$(NOVA_GATE_VERDICT="$V" bash "$G" "$TMP" 2>&1); rc=$?
if [ "$rc" -eq 1 ] && echo "$out" | grep -q "НЕ судит корпус"; then
    ok "отказ на уровне loop, с причиной"
else
    bad "уровень loop принят за достаточный (код $rc): $out"
fi

# 18. Уровень `full` — проходит: он ВЫШЕ push, а не «другой».
#     Направление «не мешать»: страж, требующий РОВНО push, запретил бы ночной полный.
echo "RC=0 SEC=3600 TIER=main:full" > "$V"
out=$(NOVA_GATE_VERDICT="$V" bash "$G" "$TMP" 2>&1); rc=$?
if [ "$rc" -eq 0 ] && echo "$out" | grep -q "слияние законно"; then
    ok "уровень full принимается как не ниже push"
else
    bad "ложный отказ на уровне full (код $rc): $out"
fi

if [ "$FAILED" -eq 0 ]; then echo "селфтест check-merge-discipline: $CASES/$CASES ok"; exit 0; fi
echo "селфтест check-merge-discipline: ЕСТЬ ПРОВАЛЫ" >&2
exit 1
