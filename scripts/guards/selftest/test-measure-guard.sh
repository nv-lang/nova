#!/usr/bin/env bash
# Селфтест scripts/tools/measure.sh — ворот против грязных замеров (№449).
#
# Страж без селфтеста не работает: ПЕРВАЯ редакция measure.sh пропустила замер
# при живом гейте (детекция шла по подстроке пути, а не по имени бинаря) — и
# поймал это именно селфтест, а не глаза. Проверяем ОБА направления:
# ловит занятость и НЕ даёт ложных срабатываний на свободной машине.
set -u
export LC_ALL=C

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
M="$ROOT/scripts/tools/measure.sh"
FAILED=0

ok()   { echo "  ok: $1"; }
bad()  { echo "  ПРОВАЛ: $1" >&2; FAILED=1; }

echo "== селфтест measure.sh =="

# 1. Ловит идущий гейт (лог есть, вердикта нет).
TMPLOG=/tmp/gate_full.log
TMPDONE=/tmp/gate_full.done
SAVED_LOG=""; SAVED_DONE=""
[ -f "$TMPLOG" ]  && { SAVED_LOG=$(mktemp);  cp "$TMPLOG" "$SAVED_LOG"; }
[ -f "$TMPDONE" ] && { SAVED_DONE=$(mktemp); cp "$TMPDONE" "$SAVED_DONE"; rm -f "$TMPDONE"; }
touch "$TMPLOG"
out=$(bash "$M" -n 1 -l selftest-gate -- true 2>&1); rc=$?
if [ "$rc" -eq 1 ] && echo "$out" | grep -q 'гейт-идёт'; then
    ok "ловит идущий гейт (код 1)"
else
    bad "НЕ поймал идущий гейт (код $rc): $out"
fi
[ -n "$SAVED_DONE" ] && { cp "$SAVED_DONE" "$TMPDONE"; rm -f "$SAVED_DONE"; }
[ -n "$SAVED_LOG" ]  && { cp "$SAVED_LOG" "$TMPLOG"; rm -f "$SAVED_LOG"; } || rm -f "$TMPLOG"

# 2. Ловит загрузку ЦП (порог опускаем до 0 — любая загрузка обязана сработать).
out=$(NOVA_MEASURE_CPU_MAX=0 bash "$M" -n 1 -l selftest-cpu -- true 2>&1); rc=$?
if [ "$rc" -eq 1 ] && echo "$out" | grep -q 'загрузка-ЦП'; then
    ok "ловит загрузку ЦП (код 1)"
else
    bad "НЕ поймал загрузку ЦП при пороге 0 (код $rc): $out"
fi

# 3. НЕ даёт ложного срабатывания на свободной машине (порог поднят до 100).
#    ВАЖНО: этот селфтест запускается ИЗНУТРИ гейта, и тогда measure.sh честно
#    видит «гейт идёт» и отказывает — тест падал не из-за дефекта инструмента, а
#    из-за собственного контекста. Симулируем завершённый гейт на время проверки
#    (маркер DONE), затем возвращаем как было.
FAKE_DONE=0
if [ -f "$TMPLOG" ] && [ ! -f "$TMPDONE" ]; then
    echo "RC=0 SEC=0 (selftest stub)" > "$TMPDONE"; FAKE_DONE=1
fi
out=$(NOVA_MEASURE_SELFTEST_NO_GATES=1 NOVA_MEASURE_CPU_MAX=100 bash "$M" -n 2 -l selftest-free -- true 2>&1); rc=$?
[ "$FAKE_DONE" -eq 1 ] && rm -f "$TMPDONE"
if [ "$rc" -eq 0 ] && echo "$out" | grep -q 'замер ВАЛИДЕН'; then
    ok "не ложно-срабатывает на свободной машине (код 0)"
else
    bad "ложное срабатывание на свободной машине (код $rc): $out"
fi

# 4. Ловит слишком большой разброс (код 2).
#    Как и случай 3: селфтест запускается ИЗНУТРИ гейта, поэтому глушим ворота
#    «гейт идёт» на время проверки — иначе measure.sh справедливо откажет ещё до
#    подсчёта разброса, и тест провалится не по своей теме.
FAKE_DONE=0
if [ -f "$TMPLOG" ] && [ ! -f "$TMPDONE" ]; then
    echo "RC=0 SEC=0 (selftest stub)" > "$TMPDONE"; FAKE_DONE=1
fi
out=$(NOVA_MEASURE_SELFTEST_NO_GATES=1 NOVA_MEASURE_CPU_MAX=100 bash "$M" -n 3 -l selftest-spread -- \
      bash -c 'if [ ! -f /tmp/_ms ]; then touch /tmp/_ms; sleep 3; else sleep 0; fi' 2>&1)
rc=$?; rm -f /tmp/_ms
[ "$FAKE_DONE" -eq 1 ] && rm -f "$TMPDONE"
if [ "$rc" -eq 2 ] && echo "$out" | grep -q 'РАЗБРОС'; then
    ok "ловит большой разброс (код 2)"
else
    bad "НЕ поймал разброс (код $rc): $out"
fi

if [ "$FAILED" -eq 0 ]; then
    echo "селфтест measure.sh: 4/4 ok"
    exit 0
fi
echo "селфтест measure.sh: ЕСТЬ ПРОВАЛЫ" >&2
exit 1
