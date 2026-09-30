#!/bin/sh
# Самотест check-novac-fuzz-zero-panic (П16). Красноту доказываем ПОДЛОЖНЫМ
# инструментом: настоящий прогон стоит секунды и уже оплачен самим стражем,
# а судить надо не фаззер, а дверь — что она пропускает зелёное, краснеет на
# красном и не молчит, когда свидетеля нет вовсе.
export LC_ALL=C
ROOT="$(cd "$(dirname "$0")/../../.." && pwd)"
G="$ROOT/scripts/guards/check-novac-fuzz-zero-panic.sh"
T="${TMPDIR:-/tmp}/selftest-fuzz-door.$$"
mkdir -p "$T"
trap 'rm -rf "$T"' 0
fails=0

printf '#!/bin/sh\necho "novac-fuzz ok: 462 mutations, panics 0"\nexit 0\n' > "$T/green.sh"
if sh "$G" "$ROOT" "$T/green.sh" >/dev/null 2>&1; then
    echo "  ok: зелёный фаззер -> зелёный страж"
else
    echo "  FAIL: зелёный фаззер дал красный" >&2; fails=$((fails+1))
fi

printf '#!/bin/sh\necho "novac-fuzz: PANIC on case basics-trunc-41" >&2\nexit 1\n' > "$T/red.sh"
if sh "$G" "$ROOT" "$T/red.sh" >/dev/null 2>&1; then
    echo "  FAIL: падение novac прошло через дверь ЗЕЛЁНЫМ" >&2; fails=$((fails+1))
else
    echo "  ok: паника -> красный"
fi

if sh "$G" "$ROOT" "$T/no-such-tool.sh" >/dev/null 2>&1; then
    echo "  FAIL: свидетеля нет, а дверь зелёная — ровно дыра F2" >&2; fails=$((fails+1))
else
    echo "  ok: нет инструмента -> красный, а не 'судить нечего'"
fi

# Бюджет не выходит за предел шага: ночной бюджет 900с при пределе 600с
# кончал фаззер обрывом без вердикта (ci-green-0930). Подложный инструмент
# печатает бюджет, который получил; при пределе 100с ждём 70.
printf '#!/bin/sh\necho "novac-fuzz ok: budget=$NOVAC_FUZZ_SECONDS"\nexit 0\n' > "$T/budget.sh"
got=$(NOVAC_TIER=full NOVAC_STEP_DEADLINE=100 sh "$G" "$ROOT" "$T/budget.sh" 2>&1)
case "$got" in
    *budget=70*) echo "  ok: бюджет урезан до 7/10 предела шага (900 -> 70 при пределе 100)" ;;
    *) echo "  FAIL: бюджет вышел за предел шага: $got" >&2; fails=$((fails+1)) ;;
esac
case "$got" in
    *"урезан с 900"*) echo "  ok: урезание названо в строке ok" ;;
    *) echo "  FAIL: бюджет урезан молча: $got" >&2; fails=$((fails+1)) ;;
esac
got=$(NOVAC_TIER=push NOVAC_STEP_DEADLINE=600 sh "$G" "$ROOT" "$T/budget.sh" 2>&1)
case "$got" in
    *budget=60*) echo "  ok: бюджет внутри предела не тронут (push 60 при пределе 600)" ;;
    *) echo "  FAIL: бюджет внутри предела изменён: $got" >&2; fails=$((fails+1)) ;;
esac

[ "$fails" -eq 0 ] && echo "test-check-novac-fuzz-zero-panic ok" && exit 0
echo "test-check-novac-fuzz-zero-panic FAIL: $fails" >&2
exit 1
