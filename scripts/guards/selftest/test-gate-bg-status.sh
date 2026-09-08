#!/bin/sh
# Самотест режима `-c` у scripts/tools/gate-bg.sh.
#
# Доказывает мутацией обе половины правки 2026-09-07:
#   * МОЯ: при живом процессе яруса novac печатается «ИДЁТ ЯРУС NOVAC», а не
#     сохранённый вердикт от прошлого прогона (правило Г9: вердикт говорит ровно
#     то, что механизм установил);
#   * nova-55: сохранённый вердикт обязан называть свой ВОЗРАСТ и отставание
#     судимого дерева от HEAD — иначе он читается как свежий и про текущий HEAD.
#
# Проба идёт В ОБЕ СТОРОНЫ у каждой половины: свежий вердикт на текущем HEAD НЕ
# должен давать предупреждения об отставании, протухший на старом хэше — обязан.
# Без второй стороны первая ничего не доказывает.
export LC_ALL=C

GD="$(cd "$(dirname "$0")/.." && pwd)"
ROOT="$(cd "$GD/../.." && pwd)"
G="$ROOT/scripts/tools/gate-bg.sh"
T="${TMPDIR:-/tmp}/gate-bg-status-selftest.$$"
mkdir -p "$T"
trap 'rm -rf "$T"; kill "$FAKE_PID" 2>/dev/null' 0

fails=0
cases=0
ok()  { echo "  ok: $1"; cases=$((cases+1)); }
bad() { echo "  FAIL: $1" >&2; fails=$((fails+1)); }

HEAD_SHA=$(git -C "$ROOT" rev-parse HEAD 2>/dev/null)
OLD_SHA=$(git -C "$ROOT" rev-parse HEAD~5 2>/dev/null)

# Шов: подставляем свои файлы и своё имя процесса, живое дерево не трогаем.
run_c() {
    GATE_BG_DONE="$1" GATE_BG_LOG="$T/empty.log" GATE_BG_STAMP="$T/stamp" \
    GATE_BG_NOVAC_PAT="${2:-selftest-no-such-process-pattern}" \
    bash "$G" -c 2>&1
}
: > "$T/empty.log"

# ── 1. свежий вердикт на ТЕКУЩЕМ HEAD — отставания нет ────────────────────
echo "RC=0 SEC=100 TIER=main:push HASH=$HEAD_SHA BRANCH=main" > "$T/fresh.done"
out=$(run_c "$T/fresh.done")
if echo "$out" | grep -q "судил ТЕКУЩИЙ HEAD"; then
    if echo "$out" | grep -q "ПОЗАДИ HEAD"; then
        bad "свежий вердикт назван отстающим"
    else
        ok "свежий вердикт на текущем HEAD — отставания не заявлено"
    fi
else
    bad "нет строки про текущий HEAD: [$(echo "$out" | tail -1)]"
fi

# ── 2. вердикт о СТАРОМ дереве — отставание названо числом ────────────────
echo "RC=0 SEC=100 TIER=main:push HASH=$OLD_SHA BRANCH=main" > "$T/stale.done"
out=$(run_c "$T/stale.done")
if echo "$out" | grep -qE "судил дерево на [0-9]+ коммит\(ов\) ПОЗАДИ HEAD"; then
    ok "протухший вердикт — отставание названо числом"
else
    bad "отставание не названо: [$(echo "$out" | tail -1)]"
fi

# ── 2а. СВЕЖЕСТЬ СТОИТ В ПЕРВОЙ строке, а не только в последней ──────
# Случаи 1 и 2 выше судят ПОДРОБНУЮ строку, и она печатается последней. Замер
# 2026-09-08: окно, которое эту строку и завело, четыре раза подряд читало вывод через
# `head -2` и её не видело — приняло вердикт девятичасовой давности за состояние машины.
# Свойство «сказано» выполнялось, «прочитано» — нет. Эти два случая режут вывод ровно так,
# как его читают, и требуют метки в заголовке.
first=$(run_c "$T/stale.done" | head -1)
if echo "$first" | grep -q "ПРОТУХШИЙ"; then
    ok "протухший вердикт назван протухшим в первой строке"
else
    bad "первая строка не называет протухания: [$first]"
fi

first=$(run_c "$T/fresh.done" | head -1)
if echo "$first" | grep -q "СВЕЖИЙ" && ! echo "$first" | grep -q "ПРОТУХШИЙ"; then
    ok "свежий вердикт назван свежим в первой строке"
else
    bad "первая строка судит свежий вердикт неверно: [$first]"
fi

# ── 2б. свежий хэш при СТАРОМ файле — случай, найденный ЖИВЫМ прогоном ────────
# Интегратор 2026-09-08: вердикт на ТЕКУЩЕМ HEAD и при этом 147 минут от роду. Он решил,
# что счётчик врёт, потому что «судил текущий HEAD» было правдой. Оси независимы: дерево
# не двигалось, а машина за два часа изменилась. Случаи 1 и 2а этой комбинации НЕ
# покрывали — обе стороны там со свежим файлом, и дыра нашлась не самотестом, а прогоном.
cp "$T/fresh.done" "$T/oldfile.done"
touch -d "@$(( $(date +%s) - 3 * 3600 ))" "$T/oldfile.done" 2>/dev/null \
    || touch -t "$(date -d '3 hours ago' +%Y%m%d%H%M 2>/dev/null)" "$T/oldfile.done" 2>/dev/null
first=$(run_c "$T/oldfile.done" | head -1)
if echo "$first" | grep -q "ПРОТУХШИЙ"; then
    ok "вердикт на текущем HEAD, но трёхчасовой — назван протухшим"
else
    bad "старый файл на текущем хэше сочтён свежим: [$first]"
fi

# ── 3. хэш, которого в дереве нет — сказано прямо ─────────────────────────
echo "RC=1 SEC=100 TIER=main:push HASH=deadbeefdeadbeefdeadbeefdeadbeefdeadbeef BRANCH=main" > "$T/unknown.done"
out=$(run_c "$T/unknown.done")
if echo "$out" | grep -q "НЕ НАЙДЕН"; then
    ok "неизвестный хэш назван неизвестным, а не выдан за совпадение"
else
    bad "молчит про неизвестный хэш: [$(echo "$out" | tail -1)]"
fi

# ── 4. ЖИВОЙ процесс яруса novac — вердикт не выдаётся за текущее состояние ─
# Случай кодирует ЗАМЕР: поднимается НАСТОЯЩИЙ процесс, чьё имя ловится тем же
# grep'ом, что и `gate-novac.sh`, а не подделывается вывод `ps`.
cat > "$T/selftest-fake-novac.sh" <<'FAKE'
sleep 12
FAKE
bash "$T/selftest-fake-novac.sh" &
FAKE_PID=$!
sleep 1
out=$(run_c "$T/fresh.done" "selftest-fake-novac")
if echo "$out" | grep -q "ИДЁТ ЯРУС NOVAC"; then
    ok "живой ярус novac виден и назван"
else
    bad "живой ярус novac не замечен: [$(echo "$out" | head -1)]"
fi

# ── 5. обратная сторона: процесса нет — строки про novac быть не должно ────
out=$(run_c "$T/fresh.done" "selftest-no-such-process-pattern")
if echo "$out" | grep -q "ИДЁТ ЯРУС NOVAC"; then
    bad "строка про novac печатается БЕЗ живого процесса"
else
    ok "без процесса строки про novac нет"
fi
kill "$FAKE_PID" 2>/dev/null

# ── 6. префиксы существующих строк не тронуты (их грепают планы и окна) ────
out=$(run_c "$T/fresh.done")
if echo "$out" | grep -qE "^ГЕЙТ ЗЕЛ"; then
    ok "префикс вердикта сохранён"
else
    bad "префикс вердикта изменён: [$(echo "$out" | head -1)]"
fi

if [ "$fails" -eq 0 ]; then
    echo "test-gate-bg-status ok: $cases случаев (счётчик, не литерал)"
    exit 0
fi
echo "test-gate-bg-status FAIL: $fails" >&2
exit 1
