#!/usr/bin/env bash
# scripts/guards/selftest/test-gate-mega-timeout-isolation.sh
# Самотест ветки мега-CU «снят пределом != красный» (правило Г16 конвенций гейта,
# docs/dev/gate-guard-conventions.md; реестр 221.1 №1038/№848).
#
# ЧТО ИМЕННО УТВЕРЖДАЕТ ВЕТКА, И ПОЧЕМУ ЭТО НЕ «СМЯГЧЕНИЕ». Раннер кладёт
# `Outcome::Timeout` в счётчик `FAIL:` (№1038), и гейт ронял шаг словом «FAIL != 0»
# ровно про то, о чём Г16 запрещает так говорить. Ветка перемеряет каждую снятую
# пределом фикстуру В ОДИНОЧКУ и по перемеру НАЗЫВАЕТ красноту:
#   в одиночку тоже снята -> настоящий повис, шаг красный;
#   в одиночку зелена     -> известный флап №848/№1038, шаг ВСЁ РАВНО красный,
#                            и отменить это может только человек словом
#                            `NOVA_GATE_TIMEOUT_ACQUIT=<причина>`.
# Вторая строка — главная и она НЕ очевидна: соблазн был зеленить автоматически.
# Нельзя: №848 подозревает у этого семейства живой замок планировщика при МАЛОМ
# числе воркеров, а нагрузка пачки — ровно это условие, так что автозелень
# оправдывала бы искомый сигнал. Случаи 2 и 3 ниже держат именно эту границу.
#
# ЗАЧЕМ САМОТЕСТ. Ветка живёт внутри `scripts/gate.sh` и исполняется только на
# полном ярусе — сорок минут чужой машины; красную её сторону «прогоном гейта» не
# увидеть вовсе (нужен настоящий повис). Поэтому самотест ВЫРЕЗАЕТ живой блок из
# gate.sh по маркерам и гоняет его на подставных логах и подставном `nova` — то
# есть судит ТОТ ЖЕ текст, что пойдёт в гейт, а не копию (копия разошлась бы на
# первой правке).
#
# Запуск: bash scripts/guards/selftest/test-gate-mega-timeout-isolation.sh
set -u

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO="$(cd "$HERE/../../.." && pwd)"
GATE="$REPO/scripts/gate.sh"
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

BEGIN_MARK='Г16 НА УРОВНЕ РАННЕРА'
END_MARK='конец ветки Г16'

BLOCK="$WORK/block.sh"
awk -v b="$BEGIN_MARK" -v e="$END_MARK" '
    index($0, b) { on = 1 }
    on { print }
    on && index($0, e) { exit }
' "$GATE" > "$BLOCK"

if [ ! -s "$BLOCK" ]; then
    echo "САМОТЕСТ СЛОМАН: блок не найден в $GATE по маркеру «$BEGIN_MARK»." >&2
    echo "Если блок переименован — почини маркер здесь, НЕ удаляй самотест." >&2
    exit 2
fi
for _need in _MEGA_REAL_FAIL _MEGA_TMO_HUNG NOVA_GATE_TIMEOUT_ACQUIT; do
    grep -q "$_need" "$BLOCK" || {
        echo "САМОТЕСТ СЛОМАН: в вырезанном блоке нет «$_need» — вырезано не то." >&2
        exit 2
    }
done
bash -n "$BLOCK" 2>/dev/null || {
    echo "САМОТЕСТ СЛОМАН: вырезанный блок не парсится — маркеры режут по живому." >&2
    exit 2
}

FAILED=0
CASES=0

# Прогон одного случая.
#   $1 имя · $2 лог · $3 сводка · $4 MEGA_EXIT · $5 вывод одиночного `nova`
#   $6 значение NOVA_GATE_TIMEOUT_ACQUIT (пусто = двери нет) · $7 ожидание PASS|FAIL
run_case() {
    local name="$1" log_body="$2" mega_line="$3" mega_exit="$4"
    local solo_out="$5" door="$6" want="$7"
    CASES=$((CASES + 1))

    local dir="$WORK/case$CASES"
    mkdir -p "$dir/spec_tests/conformance/standalone"
    # Фикстуры из подставных логов обязаны СУЩЕСТВОВАТЬ: у ветки есть отдельная
    # краснота «фикстуры нет на диске», и без файлов мы мерили бы её, а не перемер.
    printf 'fn main() -> () => ()\n' \
        > "$dir/spec_tests/conformance/standalone/tmo_one.nv"
    printf 'fn main() -> () => ()\n' \
        > "$dir/spec_tests/conformance/standalone/tmo_two.nv"

    printf '%s\n' "$log_body" > "$dir/mega.log"

    # Подставной `nova`: печатает заданную строку; код возврата — по ней.
    printf '#!/usr/bin/env bash\nprintf "%%s\\n" "%s"\ncase "%s" in *"FAIL: 0"*) exit 0;; *) exit 1;; esac\n' \
        "$solo_out" "$solo_out" > "$dir/nova"
    chmod +x "$dir/nova"

    local out rc
    out="$(
        set +e
        ESC=$'\033'
        ROOT="$dir"
        NOVA="$dir/nova"
        MEGA_LOG="$dir/mega.log"
        MEGA_LINE="$mega_line"
        MEGA_EXIT="$mega_exit"
        NOVA_GATE_TIMEOUT_ACQUIT="$door"
        export NOVA_GATE_TIMEOUT_ACQUIT
        fail() { echo "GATE FAIL: $*" >&2; exit 7; }
        # shellcheck disable=SC1090
        . "$BLOCK"
        exit 0
    2>&1)"
    rc=$?

    local got="PASS"
    [ "$rc" -eq 0 ] || got="FAIL"

    if [ "$got" = "$want" ]; then
        printf 'ok     %-52s -> %s\n' "$name" "$got"
    else
        printf 'НЕ ТАК %-52s -> %s, ожидалось %s\n' "$name" "$got" "$want"
        printf '%s\n' "$out" | sed 's/^/       | /'
        FAILED=$((FAILED + 1))
    fi
}

TMO1='TIMEOUT        spec_tests/conformance/standalone/tmo_one  # killed after 77029ms'
TMO2='TIMEOUT        spec_tests/conformance/standalone/tmo_two  # killed after 61002ms'
TMO_GONE='TIMEOUT        spec_tests/conformance/standalone/tmo_vanished  # killed after 60001ms'
REALFAIL='FAIL           spec_tests/conformance/standalone/other  # assertion'
GREEN='PASS: 1  FAIL: 0'

# 1. Здоровый прогон: ветка не должна ложнить там, где ничего не случилось.
run_case "чисто: ни одного снятого пределом" \
    "PASS: 924  FAIL: 0" "PASS: 924  FAIL: 0" 0 "$GREEN" "" PASS

# 2. ГЛАВНАЯ ГРАНИЦА — ровно случай 2026-09-08 на HASH=fc84e296b.
#    В одиночку зелена, двери нет -> шаг КРАСЕН НАМЕРЕННО.
run_case "снят пределом, в одиночку зелён, двери НЕТ" \
    "$TMO1
PASS: 924  FAIL: 1" "PASS: 924  FAIL: 1" 1 "$GREEN" "" FAIL

# 3. Та же картина, но человек назвал причину -> зелено, причина в выводе.
run_case "снят пределом, в одиночку зелён, дверь ОТКРЫТА" \
    "$TMO1
PASS: 924  FAIL: 1" "PASS: 924  FAIL: 1" 1 "$GREEN" "izvestnyy flap 848" PASS

# 4. Настоящий повис остаётся снятым и в одиночку — дверь его НЕ открывает.
run_case "в одиночку ТОЖЕ снят, дверь открыта" \
    "$TMO1
PASS: 924  FAIL: 1" "PASS: 924  FAIL: 1" 1 "PASS: 0  FAIL: 1" "izvestnyy flap 848" FAIL

# 5. ЛОВУШКА НУЛЯ: «PASS: 0 FAIL: 0» — не замер, оправданием быть не может.
run_case "перемер дал PASS: 0 FAIL: 0 (пустая выборка)" \
    "$TMO1
PASS: 924  FAIL: 1" "PASS: 924  FAIL: 1" 1 "PASS: 0  FAIL: 0" "izvestnyy flap 848" FAIL

# 6. Дверь открыта только для СНЯТЫХ ПРЕДЕЛОМ: настоящий провал рядом её не проходит.
run_case "снят пределом + настоящий провал, дверь открыта" \
    "$TMO1
$REALFAIL
PASS: 923  FAIL: 2" "PASS: 923  FAIL: 2" 1 "$GREEN" "izvestnyy flap 848" FAIL

# 7. Двое снятых, оба в одиночку зелены: без двери красно, с дверью зелено.
run_case "двое снятых, оба зелены в одиночку, двери НЕТ" \
    "$TMO1
$TMO2
PASS: 923  FAIL: 2" "PASS: 923  FAIL: 2" 1 "$GREEN" "" FAIL
run_case "двое снятых, оба зелены в одиночку, дверь ОТКРЫТА" \
    "$TMO1
$TMO2
PASS: 923  FAIL: 2" "PASS: 923  FAIL: 2" 1 "$GREEN" "izvestnyy flap 848" PASS

# 8. Отказ БЕЗ красной фикстуры (краш раннера) не должен уехать в зелень.
run_case "exit=1 при FAIL: 0 и без снятых" \
    "PASS: 924  FAIL: 0" "PASS: 924  FAIL: 0" 1 "$GREEN" "" FAIL

# 9. Дубли строк TIMEOUT (раннер печатает их дважды) не должны удваивать
#    вычитание — иначе настоящий провал ушёл бы в минус и оправдался.
run_case "строка TIMEOUT задвоена в логе" \
    "$TMO1
$REALFAIL
$TMO1
PASS: 923  FAIL: 2" "PASS: 923  FAIL: 2" 1 "$GREEN" "izvestnyy flap 848" FAIL

# 10. Имя из лога, которого нет на диске: перемерить нечем -> красно даже с дверью.
run_case "снятой фикстуры нет на диске, дверь открыта" \
    "$TMO_GONE
PASS: 924  FAIL: 1" "PASS: 924  FAIL: 1" 1 "$GREEN" "izvestnyy flap 848" FAIL

echo "----"
if [ "$FAILED" -eq 0 ]; then
    echo "САМОТЕСТ ЗЕЛЁН: $CASES/$CASES"
    exit 0
fi
echo "САМОТЕСТ КРАСЕН: провалено $FAILED из $CASES" >&2
exit 1
