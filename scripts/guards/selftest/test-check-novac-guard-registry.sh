#!/bin/sh
# Самотест check-novac-guard-registry.sh — ОБА направления (норма 254):
# страж обязан пропускать сошедшийся реестр и краснеть на каждом из четырёх
# видов расхождения по отдельности. Страж, который никогда не краснеет, —
# мёртвый механизм (класс №519), поэтому красных случаев здесь больше, чем
# зелёных, и каждый проверяется НЕ только кодом возврата, но и тем, что
# покраснел по СВОЕЙ причине (по своему разделу вывода).
#
# Подложки строятся во временном каталоге; рабочее дерево не трогается.
export LC_ALL=C
G="$(cd "$(dirname "$0")/.." && pwd)/check-novac-guard-registry.sh"
T="${TMPDIR:-/tmp}/novac-guard-registry-selftest.$$"
mkdir -p "$T" || exit 1
fails=0
ok() { echo "  ok: $1"; }
bad() { echo "  FAIL: $1" >&2; fails=$((fails + 1)); }

CLOCK=$(printf '\360\237\225\220')

# --- конструктор подложки ---------------------------------------------------
R=""
F=""
mkroot() {
    R="$T/$1"
    rm -rf "$R"
    mkdir -p "$R/docs/plans" "$R/scripts/guards/selftest"
    F="$R/docs/plans/274-novac-self-hosted-compiler.md"
    printf '%s\n' '#!/bin/sh' '# gate.sh подложки' > "$R/scripts/gate.sh"
}
guard_file() { printf '%s\n' '#!/bin/sh' 'exit 0' > "$R/scripts/guards/check-novac-$1.sh"; }
self_file()  { printf '%s\n' '#!/bin/sh' 'exit 0' > "$R/scripts/guards/selftest/test-check-novac-$1.sh"; }
gate_call()  { printf 'guard "$ROOT/scripts/guards/check-novac-%s.sh" "$ROOT" || fail "x"\n' "$1" >> "$R/scripts/gate.sh"; }
gate_note()  { printf '# guard "$ROOT/scripts/guards/check-novac-%s.sh" — упомянут только в комментарии\n' "$1" >> "$R/scripts/gate.sh"; }

plan_head() {
    {
        echo '### 10.3. Набор стражей'
        echo ''
        echo '| страж | что держит |'
        echo '|---|---|'
    } > "$F"
}
plan_mid() {
    {
        echo ''
        echo '### 10.3а. Сводный аудит: каждое правило — против своего стража'
        echo ''
        echo '| правило / требование | страж | статус |'
        echo '|---|---|---|'
    } >> "$F"
}
plan_tail() {
    {
        echo ''
        echo '### 10.4. Храповики'
        echo 'проза ЗА границей раздела: check-novac-nevermind.sh — в реестр не идёт'
    } >> "$F"
}
row() { printf '%s\n' "$1" >> "$F"; }

# Полный сошедшийся набор: стражи alpha и beta (файл + гейт + самотест +
# строка плана) плюс gamma — назван планом под маркером часов, файла нет.
# Плюс два обманки-имени: в прозе внутри §10.3 и в прозе §10.4 — ни одно из
# них в реестр попасть не должно (иначе зелёный случай покраснеет).
build_good() {
    mkroot "$1"
    guard_file alpha; guard_file beta
    self_file alpha;  self_file beta
    gate_call alpha;  gate_call beta
    plan_head
    row '| `check-novac-alpha.sh` | держит альфу |'
    row '| `check-novac-beta.sh` | держит бету |'
    row 'проза ВНУТРИ §10.3: check-novac-prosaic.sh — не строка таблицы, в реестр не идёт'
    plan_mid
    row '| правило альфы | `check-novac-alpha.sh` | в гейте |'
    row '| правило беты | `check-novac-beta.sh` | в гейте |'
    row '| правило гаммы | `check-novac-gamma.sh` | '"$CLOCK"' Э2 |'
    # Необязательный второй аргумент — лишняя строка ВНУТРИ §10.3а. Через него
    # красные случаи добавляют нарушение туда, где его судит страж: первая
    # редакция самотеста дописывала строку после build_good, та попадала за
    # заголовок §10.4 и законно игнорировалась — самотест это и поймал.
    [ -n "${2:-}" ] && row "$2"
    plan_tail
}

run() { sh "$G" "$R" > "$T/out" 2> "$T/err"; }

# --- 1. Зелёный: реестр сошёлся --------------------------------------------
build_good good
if run; then ok "сошедшийся реестр проходит"; else bad "законное покраснело: $(cat "$T/err")"; fi
[ "$(wc -l < "$T/out")" -eq 1 ] && ok "зелёный печатает РОВНО одну строку" || bad "зелёный печатает $(wc -l < "$T/out") строк, нужна одна"
grep -q 'ok:' "$T/out" && ok "зелёный печатает ok: (№645)" || bad "нет строки ok:"
grep -qF 'имён в плане §10.3/§10.3а 3 (ждут этапа под маркером 1), файлов стражей 2, вызовов в gate.sh 2, самотестов 2 (без своего стража 0), расхождений 0' "$T/out" \
    && ok "числа по всем четырём множествам верны, обманки в прозе и за §10.4 не сосчитаны" \
    || bad "числа не те: $(cat "$T/out")"

# --- 2. Красный: страж есть, но в гейте только комментарий ------------------
build_good nogate
printf '%s\n' '#!/bin/sh' '# gate.sh подложки' > "$R/scripts/gate.sh"
gate_call beta
gate_note alpha
run && bad "невызванный страж прошёл" || ok "невызванный страж пойман"
grep -q 'НЕ ВЫЗВАН В scripts/gate.sh' "$T/err" && grep -q 'check-novac-alpha.sh' "$T/err" \
    && ok "красный назвал alpha в разделе про гейт (комментарий за вызов не считается)" \
    || bad "раздел 1 не назвал alpha: $(cat "$T/err")"

# --- 3. Красный: страж есть, самотеста нет ----------------------------------
build_good noself
rm -f "$R/scripts/guards/selftest/test-check-novac-beta.sh"
run && bad "страж без самотеста прошёл" || ok "страж без самотеста пойман"
grep -q 'НЕТ САМОТЕСТА' "$T/err" && grep -q 'check-novac-beta.sh' "$T/err" \
    && ok "красный назвал beta в разделе про самотест" \
    || bad "раздел 2 не назвал beta: $(cat "$T/err")"

# --- 4. Красный: имя в плане, файла нет, маркера часов нет ------------------
build_good noclock '| правило омеги | `check-novac-omega.sh` | Э2, но без маркера |'
run && bad "имя без файла и без маркера прошло" || ok "имя без файла и без маркера поймано"
grep -q 'МАРКЕРА ЭТАПА НЕТ' "$T/err" && grep -q 'check-novac-omega.sh' "$T/err" \
    && ok "красный назвал omega в разделе про план" \
    || bad "раздел 3 не назвал omega: $(cat "$T/err")"
grep -q 'check-novac-gamma.sh' "$T/err" \
    && bad "gamma под маркером часов ошибочно объявлена нарушением" \
    || ok "gamma под маркером часов нарушением не считается"

# --- 5. Красный: страж есть, строки в аудите нет ----------------------------
build_good noaudit
guard_file delta; self_file delta; gate_call delta
run && bad "страж вне аудита прошёл" || ok "страж вне аудита пойман"
grep -q 'НЕТ В АУДИТЕ' "$T/err" && grep -q 'check-novac-delta.sh' "$T/err" \
    && ok "красный назвал delta в разделе про аудит" \
    || bad "раздел 4 не назвал delta: $(cat "$T/err")"

# --- 6. Зелёный: самотест без своего стража — не красный, но сосчитан -------
build_good orphan
self_file zeta
run && ok "групповой самотест без своего стража не краснит" || bad "самотест-сирота покраснел: $(cat "$T/err")"
grep -qF 'без своего стража 1' "$T/out" && ok "самотест-сирота сосчитан числом" || bad "сирота не сосчитан: $(cat "$T/out")"

# --- 7. Красный: раздел плана переименован — страж обязан ослепнуть с шумом -
build_good blind
plan_head
row '| `check-novac-alpha.sh` | держит альфу |'
plan_tail
run && bad "план без §10.3а прошёл — страж ослеп молча (класс №519)" || ok "план без второго раздела пойман"
grep -q "заголовков" "$T/err" && ok "красный объяснил, что разделы не найдены" || bad "нет объяснения про заголовки: $(cat "$T/err")"

# --- 8. Красный: плана нет вовсе --------------------------------------------
build_good noplan
rm -f "$F"
run && bad "отсутствие плана прошло" || ok "отсутствие плана поймано"

rm -rf "$T"
if [ "$fails" -eq 0 ]; then
    echo "test-check-novac-guard-registry ok: 8 случаев (2 зелёных, 6 красных), 18/18 ассертов"
    exit 0
fi
echo "test-check-novac-guard-registry: FAIL ($fails)" >&2
exit 1
