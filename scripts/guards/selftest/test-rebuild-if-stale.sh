#!/usr/bin/env bash
# Селфтест scripts/tools/rebuild-if-stale.sh — режим --check (сборку не гоняем:
# она идёт больше минуты и в селфтесте неуместна).
set -u
export LC_ALL=C
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
G="$ROOT/scripts/tools/rebuild-if-stale.sh"
FAILED=0
ok()  { echo "  ok: $1"; }
bad() { echo "  ПРОВАЛ: $1" >&2; FAILED=1; }

# 1. На реальном дереве --check обязан отвечать однозначно (0 или 1), без падений.
out=$(bash "$G" --check 2>&1); rc=$?
if [ "$rc" -eq 0 ] || [ "$rc" -eq 1 ]; then
    ok "--check даёт определённый вердикт (код $rc)"
else
    bad "--check упал с кодом $rc: $out"
fi

# 2. Вердикт обязан быть НАЗВАН словами, а не только кодом — иначе в логе гейта
#    его не отличить от молчания.
if echo "$out" | grep -qE 'свежий|УСТАРЕЛ|не тронут'; then
    ok "вердикт назван словами"
else
    bad "вердикт не назван: $out"
fi

# 3. Неизвестный аргумент — отказ, а не молчаливое игнорирование.
out=$(bash "$G" --нет-такого 2>&1); rc=$?
if [ "$rc" -eq 1 ] && echo "$out" | grep -q 'неизвестный аргумент'; then
    ok "неизвестный аргумент отвергается"
else
    bad "неизвестный аргумент проглочен (код $rc): $out"
fi

if [ "$FAILED" -eq 0 ]; then echo "селфтест rebuild-if-stale: 3/3 ok"; exit 0; fi
echo "селфтест rebuild-if-stale: ЕСТЬ ПРОВАЛЫ" >&2
exit 1
