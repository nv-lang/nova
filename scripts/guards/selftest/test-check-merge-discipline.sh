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
ok()  { echo "  ok: $1"; }
bad() { echo "  ПРОВАЛ: $1" >&2; FAILED=1; }

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

# 3. Вердикт зелёный и свежий — пропуск. Это направление важнее первых двух:
#    страж, отказывающий всегда, будет обойдён и правило умрёт.
echo "RC=0 SEC=2412" > "$V"
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

if [ "$FAILED" -eq 0 ]; then echo "селфтест check-merge-discipline: 6/6 ok"; exit 0; fi
echo "селфтест check-merge-discipline: ЕСТЬ ПРОВАЛЫ" >&2
exit 1
