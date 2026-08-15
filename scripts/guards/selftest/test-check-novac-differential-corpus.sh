#!/usr/bin/env bash
# Самотест КОРПУСНОЙ половины check-novac-differential.sh — храповика НА
# ПРОГРЕСС (план 274 §10.4). Норма самотестов — план 231 §4в.
#
# ЗАЧЕМ ОТДЕЛЬНЫЙ ФАЙЛ. У стража две половины. Фикстурную (исход novac против
# оракула на novac/fixtures/pos_*.nv) уже судят test-check-novac-differential.sh
# и test-check-novac-binary-guards.sh. Корпусная — та, что читает машинную
# строку scripts/tools/novac-diff-corpus.sh и сверяет числа с
# scripts/guards/novac-corpus.baseline В ОБЕ СТОРОНЫ — до сих пор не была
# покрыта ничем, то есть главный храповик плана 274 держался на слово.
#
# ПОДЛОЖКА. Корпусная половина достижима только через зелёную фикстурную,
# поэтому временный корень несёт всё сразу: одну фикстуру pos_*.nv,
# поддельного оракула по фиксированному пути nova-cli/target/release/nova.exe,
# поддельный бинарь novac (шов $2 стража), поддельный раннер
# scripts/tools/novac-diff-corpus.sh (печатает заготовленный текст и выходит
# заготовленным кодом), базу храповика и базу цены. Настоящий прогон корпуса
# идёт минуты — судить надо ЛОГИКУ сверки чисел, а не корпус.
#
# Копия lib/novac.sh кладётся в подложку намеренно (см. тот же приём в
# test-check-novac-iteration-cost.sh).
set -u
export LC_ALL=C
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
G="$ROOT/scripts/guards/check-novac-differential.sh"
TMP="$(mktemp -d)"; trap 'rm -rf "$TMP"' EXIT
PASS=0; FAIL=0
ok()  { PASS=$((PASS+1)); echo "  ok   $1"; }
bad() { FAIL=$((FAIL+1)); echo "  FAIL $1" >&2; }
check(){ if [ "$2" = "$3" ]; then ok "$1"; else bad "$1 (ждал '$3', получил '$2')"; fi; }
has()  { if grep -q "$2" "$1"; then ok "$3"; else bad "$3 (нет '$2' в $1: $(tr '\n' '|' < "$1"))"; fi; }

FIX="$TMP/root"
mkdir -p "$FIX/novac/fixtures" "$FIX/nova-cli/target/release" \
         "$FIX/scripts/tools" "$FIX/scripts/guards/lib"
cp "$ROOT/scripts/guards/lib/novac.sh" "$FIX/scripts/guards/lib/novac.sh"
printf 'fn main() {}\n' > "$FIX/novac/fixtures/pos_a.nv"

# Оракул и novac согласны на единственной фикстуре — фикстурная половина
# зелёная, значит доходим до корпусной.
printf '#!/bin/sh\nexit 0\n' > "$FIX/nova-cli/target/release/nova.exe"
chmod +x "$FIX/nova-cli/target/release/nova.exe"
BIN="$TMP/novac.exe"
printf '#!/bin/sh\nexit 0\n' > "$BIN"; chmod +x "$BIN"

# Поддельный дифф-раннер: печатает заготовку, выходит заготовленным кодом.
cat > "$FIX/scripts/tools/novac-diff-corpus.sh" <<'FAKE'
#!/bin/sh
R="$(cd "$(dirname "$0")/../.." && pwd)"
cat "$R/corpus.out.fixture"
exit "$(cat "$R/corpus.rc.fixture")"
FAKE
chmod +x "$FIX/scripts/tools/novac-diff-corpus.sh"

BASE="$FIX/scripts/guards/novac-corpus.baseline"
COST="$FIX/scripts/guards/novac-iteration-cost.baseline"
# Обманка печатает ТУ ЖЕ форму строк, что настоящий раннер (последние строки
# scripts/tools/novac-diff-corpus.sh): страж читает их якорными grep/sed, и
# обманка, разошедшаяся с реальностью, красила бы самотест зелёным на форме,
# которой в природе нет — та же дыра, что закрывает F9, только этажом ниже.
# Что форма не разошлась, судит блок «обманка не разошлась с раннером» ниже.
# $1 contract-match, $2 behavior-match, $3 стена прогона (мс)
corpus_out() {
    {
        printf 'novac-diff-corpus: oracle-pin=c5a0bc425 oracle-HEAD=abcdef123 spec-point=2026-08-14 spec-queue=0 (в nova.toml: 0) сборка novac=single-file корпус=examples\n'
        printf 'novac-diff-corpus: файлов 60 — совпали-приняли %s · совпали-отвергли 0 · отставание 40 · вне-точки 0 · заблокировано-оракулом 9 · DANGER 0 · PANIC 0 · allow 0\n' "$1"
        printf 'novac-diff-corpus: поведенчески совпали %s из %s · самосборка: отвергнуто 0 из 18\n' "$2" "$1"
        printf 'novac-diff-corpus: цена прогона — novac 40000ms, оракул 20000ms, стена %sms\n' "$3"
        printf 'novac-diff-corpus baseline-numbers: contract-match=%s behavior-match=%s out-of-point=0 oracle-blocked=9 self-distance=0/18\n' "$1" "$2"
        printf 'novac-diff-corpus ok\n'
    } > "$FIX/corpus.out.fixture"
}
mkrc()   { printf '%s\n' "$1" > "$FIX/corpus.rc.fixture"; }
mkbase() { printf 'contract-match %s\nbehavior-match %s\n' "$1" "$2" > "$BASE"; }
mkcost() { printf 'diff-corpus-ms %s\n' "$1" > "$COST"; }
run() { NOVAC_CORPUS=1 sh "$G" "$FIX" "$BIN" > "$TMP/out" 2> "$TMP/err"; echo $?; }

mkrc 0; corpus_out 11 5 68000; mkbase 11 5; mkcost 140000

echo "== обманка не разошлась с настоящим раннером =="
REAL="$ROOT/scripts/tools/novac-diff-corpus.sh"
hasF() { if grep -qF "$2" "$1"; then ok "$3"; else bad "$3 (нет '$2' в $1)"; fi; }
hasE() { if grep -qE "$2" "$1"; then ok "$3"; else bad "$3 (не нашёл /$2/ в $1)"; fi; }
hasF "$REAL" 'novac-diff-corpus baseline-numbers: contract-match=' "машинная строка зовётся так же, как в обманке"
hasF "$REAL" 'behavior-match=' "второе число храповика зовётся так же"
hasF "$REAL" 'out-of-point=' "корзина «вне точки» зовётся так же"
hasF "$REAL" 'self-distance=' "хвост машинной строки тот же"
hasF "$REAL" 'novac-diff-corpus: цена прогона' "строка цены зовётся так же"
hasE "$REAL" 'стена .*ms' "стена печатается в мс — её и парсит бюджет П14"
hasF "$REAL" 'novac-diff-corpus: поведенчески совпали' "строка поведения зовётся так же"

echo "== дверь: без бинаря судить нечего, с исходником — нет =="
check "бинаря нет, novac/src/main.nv нет — зелёный" \
      "$(NOVAC_CORPUS=1 sh "$G" "$FIX" "$TMP/absent" > "$TMP/out" 2> "$TMP/err"; echo $?)" "0"
has "$TMP/out" 'ok: судить нечего' "«судить нечего» напечатано (№645)"
mkdir -p "$FIX/novac/src"; printf 'fn main() {}\n' > "$FIX/novac/src/main.nv"
check "novac/src/main.nv есть, бинаря нет — красный (274.3/F1)" \
      "$(NOVAC_CORPUS=1 sh "$G" "$FIX" "$TMP/absent" > "$TMP/out" 2> "$TMP/err"; echo $?)" "1"
rm -rf "$FIX/novac/src"

echo "== числа равны базе — проходит =="
check "contract/behavior == база — зелёный" "$(run)" "0"
has "$TMP/out" 'исходы совпали с оракулом' "фикстурная половина пройдена (иначе до корпуса не дойти)"
has "$TMP/out" 'ok: храповик корпуса' "корпусная зелёная строка"
has "$TMP/out" '== база' "равенство базе названо"
has "$TMP/out" 'поведенчески совпали' "корзины раннера напечатаны рядом с базой"

echo "== ловит откат и рост =="
corpus_out 10 5 68000
check "contract МЕНЬШЕ базы — красный (откат)" "$(run)" "1"
has "$TMP/err" 'ОТКАТ' "откат назван откатом"

corpus_out 11 4 68000
check "behavior МЕНЬШЕ базы — красный (откат)" "$(run)" "1"
has "$TMP/err" 'ОТКАТ' "откат назван и по второму числу"

corpus_out 12 5 68000
check "contract БОЛЬШЕ базы — красный (рост без поднятия базы)" "$(run)" "1"
has "$TMP/err" 'без поднятия базы' "рост назван ростом"
has "$TMP/err" 'novac-corpus.baseline' "подсказка «как чинить» называет файл базы"

corpus_out 11 6 68000
check "behavior БОЛЬШЕ базы — красный" "$(run)" "1"

corpus_out 12 6 68000; mkbase 12 6
check "числа выросли И база поднята тем же коммитом — зелёный" "$(run)" "0"
mkbase 11 5; corpus_out 11 5 68000

echo "== ловит непарсимое =="
printf 'novac-diff-corpus baseline-numbers: contract-match=? behavior-match=?\n' > "$FIX/corpus.out.fixture"
check "строка есть, чисел нет — красный" "$(run)" "1"
has "$TMP/err" 'не распарсил числа' "непарсимость названа"

printf 'novac-diff-corpus: прогон без машинной строки\n' > "$FIX/corpus.out.fixture"
check "машинной строки нет вовсе — красный" "$(run)" "1"
has "$TMP/err" 'не распарсил числа' "отсутствие строки названо тем же вердиктом"

corpus_out 11 5 68000
printf 'contract-match одиннадцать\nbehavior-match пять\n' > "$BASE"
check "числа в БАЗЕ не числа — красный" "$(run)" "1"
has "$TMP/err" 'не распарсил числа' "непарсимая база названа"
mkbase 11 5

# Граничный зелёный: база, пришедшая из checkout с core.autocrlf=true. Страж
# чистит CR перед разбором — числа обязаны совпасть, а не «не распарситься».
printf 'contract-match 11\r\nbehavior-match 5\r\n' > "$BASE"
check "база с CRLF (реальность Windows) — зелёный, а не «не распарсил»" "$(run)" "0"
mkbase 11 5

echo "== раннер упал / цена вышла из бюджета =="
mkrc 3
check "раннер вернул не 0 — красный" "$(run)" "1"
has "$TMP/err" 'корпусный прогон красный' "падение раннера названо"
mkrc 0

corpus_out 11 5 200000
check "стена раннера > бюджета diff-corpus-ms — красный (П14)" "$(run)" "1"
has "$TMP/err" 'ПРОСАДКА цены дифф-раннера' "просадка цены названа"

rm -f "$COST"
check "базы цены нет — цена не судится, храповик зелёный" "$(run)" "0"
mkcost 140000; corpus_out 11 5 68000

echo "== выключатели =="
rm -f "$BASE"
check "базы храповика нет — зелёный (Э1: храповика ещё нет)" "$(run)" "0"
has "$TMP/out" 'храповика ещё нет' "отсутствие базы названо честной строкой"
mkbase 11 5

mkrc 1; corpus_out 10 4 200000
check "NOVAC_CORPUS=0 — зелёный, раннер не зовётся" \
      "$(NOVAC_CORPUS=0 sh "$G" "$FIX" "$BIN" > "$TMP/out" 2> "$TMP/err"; echo $?)" "0"
has "$TMP/out" 'корпусная часть пропущена' "пропуск назван строкой"

echo "итог: $PASS ok, $FAIL FAIL"
if [ "$FAIL" -eq 0 ]; then
    echo "test-check-novac-differential-corpus ok: $PASS/$PASS"
    exit 0
fi
exit 1
