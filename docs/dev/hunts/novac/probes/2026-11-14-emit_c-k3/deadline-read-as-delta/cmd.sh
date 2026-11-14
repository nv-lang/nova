#!/usr/bin/env bash
# deadline-read-as-delta (emit_c x K3, hunt 2026-11-14-emit_c-k3).
#
# Вопрос, на который два места отвечают по-разному:
#   "что означает второй именованный аргумент supervised -- точку или интервал?"
#
#   - std/src/prelude/errors.nv (документированная форма D442/D349):
#     `supervised(deadline: Monotonic.now() + 200.to_millis())` -- АБСОЛЮТНАЯ
#     точка монотонной шкалы.
#   - novac/src/emit_c/emit_supervised.nv:63 -- `timeout` и `deadline` читаются
#     ОДИНАКОВО; :91 печатает для обоих
#     `deadline_ns = nova_deadline_combine(..., time_monotonic_ns() + EXPR.nanos)`
#     -- форму ДЕЛЬТЫ. Абсолютная точка из прошлого становится будущим.
#   Оракул ключуется именем аргумента: timeout: -- дельта (now + .nanos),
#   deadline: -- абсолютная точка (.nanos без now), compiler-codegen
#   emit_c.rs, ветка `if dl.relative`.
#
# Замер 2026-11-14 (оба компилятора, один файл probe.nv):
#   оракул -> -1 (дедлайн в прошлом сработал до тела), exit 0;
#   novac  -> 7 и полное тело (1 с сна) -- поведенческий дифференциал.
#
# Запуск: cmd.sh <корень>; exit 0 = свидетели на месте (check принял
# `deadline:`, а C комбинирует аргумент с now как дельту).
set -u
root="${1:-.}"
P="$(cd "$(dirname "$0")" && pwd)"
NOVAC="$root/novac/target/novac.exe"
[ -f "$NOVAC" ] || { echo "NO $NOVAC (собери novac)"; exit 2; }
export NOVA_STD_PATH="$root/std/src"
T="${TMPDIR:-/tmp}/hunt-deadline.$$"
mkdir -p "$T"; trap 'rm -rf "$T"' 0
fail=0

# якорь в самом носителе: аргумент называется deadline:
grep -q 'supervised(deadline:' "$P/probe.nv" && echo "witness: носитель передаёт deadline:" \
    || { echo "ANCHOR LOST: в probe.nv нет supervised(deadline:"; fail=1; }

"$NOVAC" check "$P/probe.nv" >"$T/chk" 2>&1
[ $? -eq 0 ] && echo "check: принял (rc 0)" || { echo "ANCHOR LOST: check не принял"; cat "$T/chk"; exit 1; }
"$NOVAC" emit "$P/probe.nv" >"$T/p.c" 2>"$T/p.err"
[ $? -eq 0 ] && echo "emit: rc 0" || { echo "ANCHOR LOST: emit rc != 0"; cat "$T/p.err"; exit 1; }

# СВИДЕТЕЛЬ РАСХОЖДЕНИЯ: дельта-комбинирование для аргумента deadline:
# time_monotonic_ns() + (int64_t)(<точка>).nanos
grep -q 'deadline_ns = nova_deadline_combine(.*deadline_ns, time_monotonic_ns() + (int64_t)(' "$T/p.c" \
    && echo "witness: deadline-аргумент комбинирован как ДЕЛЬТА (now + точка)" \
    || { echo "ANCHOR LOST: дельта-форма не найдена -- расхождение ушло"; fail=1; }

# И этих строк НЕ должно быть в стабильном виде: absolut-форма оракула --
# combine(..., (int64_t)(EXPR.nanos)) БЕЗ now -- на этой эмиссии отсутствует.
grep -q 'nova_deadline_combine(.*deadline_ns, (int64_t)(' "$T/p.c" \
    && { echo "ANCHOR LOST: нашлась absolut-форма -- novac уже различает имена"; fail=1; }

# якорь в спеке-примере прелюдии: документированная форма -- абсолютная точка
grep -q 'supervised(deadline: Monotonic.now() + 200.to_millis())' "$root/std/src/prelude/errors.nv" \
    && echo "witness: prelude документирует deadline: абсолютной точкой" \
    || { echo "ANCHOR LOST: в std/src/prelude/errors.nv нет формы deadline: Monotonic.now() + ..."; fail=1; }

[ "$fail" -eq 0 ] && echo "PROBE OK: deadline: читается как дельта, спека и оракул читают её точкой" || echo "PROBE RED"
exit "$fail"
