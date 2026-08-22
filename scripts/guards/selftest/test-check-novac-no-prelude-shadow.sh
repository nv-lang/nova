#!/usr/bin/env bash
# Самотест check-novac-no-prelude-shadow.py — запрет тени прелюдных имён в
# novac (план 274 §10.3; норма самотестов — план 231 §4в: ловит нарушение
# И не даёт ложняка).
#
# ПОДЛОЖКА. У стража два шва: $2 — директория novac, $3 — директория
# прелюдии. Поэтому настоящие деревья не нужны: каждый случай — своя пара
# крошечных деревьев .nv во временном каталоге.
#
# ЧТО ЗДЕСЬ ВАЖНО ПРОВЕРИТЬ ОСОБО. Два разных провала стоят рядом:
#   * пропуск — тень типа/функции не увидена (ради этого страж и написан);
#   * ЛОЖНЯК — метод `fn Тип @имя(` носит то же слово, что прелюдный тип
#     или функция, но живёт в другом пространстве имён; если страж
#     покраснеет на нём, его выключат в первый же день.
# И ещё одно: список имён прелюдии страж ВЫВОДИТ ИЗ ДАННЫХ (её `export`),
# а не держит зашитым — самотест обязан это доказать: слово зелёное, пока
# прелюдия его не экспортирует, и красное сразу, как только появилось.
#
# Разбор у стража ОДИН на обе стороны, и здесь это закреплено попарно: одна
# и та же форма декларации проверяется и как имя прелюдии, и как декларация
# novac (generic-голова с пробелом, CRLF, `*_test.nv` с обеих сторон).
# Расхождение разбора — это «зелено там, красно тут», то есть либо ложняк,
# либо молчаливая дыра, и ловится оно только парным случаем.
set -u
export LC_ALL=C
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
G="$ROOT/scripts/guards/check-novac-no-prelude-shadow.py"
TMP="$(mktemp -d)"; trap 'rm -rf "$TMP"' EXIT
PASS=0; FAIL=0
ok()  { PASS=$((PASS+1)); echo "  ok   $1"; }
bad() { FAIL=$((FAIL+1)); echo "  FAIL $1" >&2; }
check(){ if [ "$2" = "$3" ]; then ok "$1"; else bad "$1 (ждал '$3', получил '$2')"; fi; }
has()  { if grep -q "$2" "$1"; then ok "$3"; else bad "$3 (нет '$2' в $1: $(tr '\n' '|' < "$1"))"; fi; }

SRC="$TMP/src"; PRE="$TMP/prelude"
mkdir -p "$SRC/lex" "$SRC/sem" "$PRE"
run() { python "$G" "$ROOT" "$SRC" "$PRE" > "$TMP/out" 2> "$TMP/err"; echo $?; }
# Сколько имён прелюдии страж насчитал в последнем зелёном прогоне.
nnames() { sed -n 's/.*имён прелюдии: \([0-9][0-9]*\),.*/\1/p' "$TMP/out"; }

# Прелюдия-подложка: два типа, свободная функция, свободная функция с
# generic-головой из двух параметров (пробел после запятой — форма, на
# которой разбор по полям строки молча терял имя), один метод и одна
# ассоциированная функция (последние две — НЕ имена прелюдного простран-
# ства), плюс неэкспортированный тип (тоже не имя прелюдии).
clean_prelude() {
    rm -f "$PRE"/*.nv
    cat > "$PRE/core.nv" <<'EOF'
export type Outcome[T] enum Finished(T) | Aborted
export type Error {
    message str
}
export fn outcome() -> int {
    return 1
}
export fn zip[T, U](a T, b U) -> int => 0
export fn Error.new(message str) -> Self => { message: message }
export fn Error @text() -> str => @message
type Hidden {
    x int
}
EOF
}

# Чистое дерево novac: имена по роли внутри компилятора.
clean_novac() {
    rm -rf "${SRC:?}"/*; mkdir -p "$SRC/lex" "$SRC/sem"
    cat > "$SRC/lex/lex.nv" <<'EOF'
export type Token {
    kind int
}
export fn lex(src str) -> []Token {
    return Vec[Token].of()
}
EOF
    cat > "$SRC/sem/check.nv" <<'EOF'
type Cursor {
    at int
}
fn Cursor mut @take() -> int => @at
fn is_space(b u8) -> bool => b == 32
EOF
}

echo "== судить нечего =="
python "$G" "$ROOT" "$TMP/absent" "$PRE" > "$TMP/out" 2>&1
check "нет директории novac — зелёный" "$?" "0"
has "$TMP/out" 'ok: судить нечего' "«судить нечего» напечатано (№645)"
python "$G" "$ROOT" "$SRC" "$TMP/absent-prelude" > "$TMP/out" 2>&1
check "нет директории прелюдии — зелёный" "$?" "0"
has "$TMP/out" 'ok: судить нечего' "«судить нечего» напечатано и про прелюдию"

echo "== чистая пара деревьев =="
clean_prelude; clean_novac
check "имена novac не пересекаются с прелюдией — зелёный" "$(run)" "0"
has "$TMP/out" 'имён прелюдии' "число имён прелюдии напечатано"
has "$TMP/out" 'теней: 0' "итог напечатан числом"

echo "== ловит тень типа (живой случай 2026-08-15) =="
clean_novac
printf 'type Outcome enum Done | Failed\n' >> "$SRC/sem/check.nv"
check "type Outcome при прелюдном Outcome[T] — красный" "$(run)" "1"
has "$TMP/err" 'Outcome' "имя-нарушитель названо"
has "$TMP/err" 'sem/check.nv:' "файл:строка нарушителя названы"
has "$TMP/err" 'core.nv:1' "названо и место в прелюдии, откуда имя"
has "$TMP/err" 'переименовать' "подсказка «как чинить» есть"

clean_novac
printf 'export type Outcome[T] enum Done(T) | Failed\n' >> "$SRC/sem/check.nv"
check "export type Outcome[T] — тот же красный (generic-хвост не прячет имя)" "$(run)" "1"

echo "== ловит тень свободной функции =="
clean_novac
printf 'fn outcome(x int) -> int {\n    return x\n}\n' >> "$SRC/sem/check.nv"
check "fn outcome( при прелюдной fn outcome( — красный" "$(run)" "1"
has "$TMP/err" 'outcome' "имя функции названо"
clean_novac
printf 'export fn outcome() -> int => 7\n' >> "$SRC/lex/lex.nv"
check "export fn outcome() — тоже красный" "$(run)" "1"

echo "== generic-голова из двух параметров: разбор один с обеих сторон =="
# Разбор по полям строки терял такое имя МОЛЧА и со стороны прелюдии, и со
# стороны novac: поле обрывалось на пробеле внутри квадратных скобок, имя в
# список не попадало, и настоящая тень проходила зелёной.
clean_novac
printf 'fn zip[T, U](a T, b U) -> int => 1\n' >> "$SRC/sem/check.nv"
check "fn zip[T, U]( при такой же прелюдной — красный" "$(run)" "1"
has "$TMP/err" 'fn zip' "имя generic-функции названо"
clean_novac
printf 'export fn zip[T, U](a T, b U) -> int => 1\n' >> "$SRC/lex/lex.nv"
check "export fn zip[T, U]( — тот же красный" "$(run)" "1"
clean_novac
printf 'fn zipper[T, U](a T, b U) -> int => 1\n' >> "$SRC/sem/check.nv"
check "fn zipper[T, U]( — ЗЕЛЁНЫЙ (совпадение целиком, не по началу)" "$(run)" "0"

echo "== НЕ ложняк: методы и ассоциированные функции =="
clean_novac
printf 'fn str @outcome() -> int => 1\n' >> "$SRC/sem/check.nv"
check "метод fn str @outcome() — ЗЕЛЁНЫЙ (иное пространство имён)" "$(run)" "0"
clean_novac
printf 'fn Cursor mut @outcome() -> int => @at\n' >> "$SRC/sem/check.nv"
check "метод с mut — ЗЕЛЁНЫЙ" "$(run)" "0"
clean_novac
printf 'fn Cursor.outcome(at int) -> Self => { at: at }\n' >> "$SRC/sem/check.nv"
check "ассоциированная fn Cursor.outcome( — ЗЕЛЁНЫЙ (имя за типом)" "$(run)" "0"

echo "== НЕ ложняк: novac навешивает своё на ПРЕЛЮДНЫЙ тип =="
# Обычное дело: свой метод на прелюдном Outcome[T]. Имя типа стоит в позиции
# получателя, а не объявляется заново — тени нет.
clean_novac
printf 'fn Outcome[T] @describe() -> str => "outcome"\n' >> "$SRC/sem/check.nv"
check "метод на generic-получателе fn Outcome[T] @describe( — ЗЕЛЁНЫЙ" "$(run)" "0"
clean_novac
printf 'export fn Outcome[T] mut @reset() -> int => 0\n' >> "$SRC/sem/check.nv"
check "export-метод на generic-получателе с mut — ЗЕЛЁНЫЙ" "$(run)" "0"
clean_novac
printf 'fn Outcome[T].aborted() -> Self => Outcome.Aborted\n' >> "$SRC/sem/check.nv"
check "ассоциированная fn Outcome[T].aborted( — ЗЕЛЁНЫЙ" "$(run)" "0"

echo "== НЕ ложняк: имя лишь похоже, и не всякая строка прелюдии — имя =="
clean_novac
printf 'type OutcomeKind enum A | B\nfn outcome_of(x int) -> int => x\n' >> "$SRC/sem/check.nv"
check "OutcomeKind / outcome_of — ЗЕЛЁНЫЙ (совпадение целиком, не по куску)" "$(run)" "0"
clean_novac
printf 'type Hidden {\n    x int\n}\n' >> "$SRC/sem/check.nv"
check "имя НЕэкспортированного типа прелюдии — ЗЕЛЁНЫЙ (тенить нечего)" "$(run)" "0"
clean_novac
printf '// type Outcome enum Done | Failed -- renamed to StepResult\n// fn outcome() -> int => 0\n' >> "$SRC/sem/check.nv"
check "снятая в комментарий декларация в столбце 0 — ЗЕЛЁНЫЙ" "$(run)" "0"

echo "== CRLF: тень видна и в файле с виндовыми концами строк =="
clean_novac
printf 'type Outcome enum Done | Failed\r\n' >> "$SRC/sem/check.nv"
check "CRLF-файл с тенью — красный (а не зелёный по недосмотру)" "$(run)" "1"
has "$TMP/err" 'type Outcome ' "имя названо без мусорного CR внутри"
clean_prelude
printf 'export type Frobber {\r\n    x int\r\n}\r\n' >> "$PRE/core.nv"
clean_novac
printf 'type Frobber {\n    x int\n}\n' >> "$SRC/sem/check.nv"
check "CRLF в самой прелюдии — имя всё равно прочитано, красный" "$(run)" "1"
clean_prelude

echo "== тесты исключены с ОБЕИХ сторон =="
clean_novac
printf 'type Outcome enum Done | Failed\n' > "$SRC/sem/check_test.nv"
check "то же имя в novac/*_test.nv — ЗЕЛЁНЫЙ" "$(run)" "0"
# Файл прелюдии *_test.nv объявляет СВОЙ модуль (prelude.embed_test, а не
# prelude.embed): его export в автоимпорт не попадает, и считать это слово
# именем прелюдии значило бы красить novac зря. Ложняк был живым до
# 2026-08-16 — исключение тестов стояло только на стороне novac.
clean_prelude; clean_novac
check "опора: чистая пара зелёная" "$(run)" "0"
BASE_NAMES="$(nnames)"
cat > "$PRE/embed_test.nv" <<'EOF'
module prelude.embed_test
export type FakeDir {
    x int
}
EOF
printf 'type FakeDir {\n    x int\n}\n' >> "$SRC/sem/check.nv"
check "имя из prelude/*_test.nv — ЗЕЛЁНЫЙ (не имя автоимпорта)" "$(run)" "0"
check "и в счёт имён прелюдии оно не попало" "$(nnames)" "$BASE_NAMES"
clean_prelude

echo "== выводимость списка из прелюдии (а не зашит) =="
clean_prelude; clean_novac
printf 'type Frobnicate {\n    x int\n}\n' >> "$SRC/sem/check.nv"
check "имени нет в прелюдии — зелёный" "$(run)" "0"
printf 'export type Frobnicate {\n    x int\n}\n' >> "$PRE/core.nv"
check "то же имя появилось в прелюдии — красный БЕЗ правки стража" "$(run)" "1"
has "$TMP/err" 'Frobnicate' "новое имя названо поимённо"
clean_prelude

echo "== настоящее дерево =="
python "$G" "$ROOT" >/dev/null 2>&1
check "novac/src проекта не тенит прелюдию" "$?" "0"

echo "итог: $PASS ok, $FAIL FAIL"
if [ "$FAIL" -eq 0 ]; then
    echo "test-check-novac-no-prelude-shadow ok: $PASS/$PASS"
    exit 0
fi
exit 1
