#!/usr/bin/env bash
# SPDX-License-Identifier: MIT OR Apache-2.0
# Селфтест фазы EMIT мутационного фаззера (scripts/tools/novac-fuzz-mutations.sh).
#
# ЗАЧЕМ ОН ЕСТЬ. Фаза emit добавлена 2026-09-09, потому что приёмка Э1 ранга CORE
# («novac не падает ни на одном входе») кончалась на `check`: мутация, прошедшая
# проверку типов, объявлялась зелёной и до эмиссии не доходила. Сама фаза без
# самотеста была бы недоказуема: соседний `test-check-novac-fuzz-zero-panic.sh`
# подменяет ИНСТРУМЕНТ целиком и потому о его внутренностях не знает ничего.
#
# ЧТО ИМЕННО ДОКАЗЫВАЕТСЯ — классификация красного случая по ЧЕТЫРЁМ корзинам, и
# доказывается в обе стороны: каждая корзина проверяется входом, который в неё
# обязан попасть, И тем, что соседняя на нём НЕ срабатывает. Корзины разведены не
# ради красоты отчёта: третью назвало окно 274, и без неё их же случай
# (`println('z')` — форма законна, чекер прав, ICE у эмиттера) уехал бы в «шум».
#
# КАК: `NOVAC_BIN`/`ORACLE_BIN` — шов, заведённый той же правкой. Подставляем
# заглушки-скрипты, ведущие себя по заданному сценарию, и читаем имя корзины.
# Настоящие бинари не нужны: судится ЛОГИКА разведения, а не компилятор.
set -u

ROOT="$(cd "$(dirname "$0")/../../.." && pwd)"
TOOL="$ROOT/scripts/tools/novac-fuzz-mutations.sh"
T="$(mktemp -d)"
trap 'rm -rf "$T"' EXIT
fails=0

[ -f "$TOOL" ] || { echo "нет $TOOL" >&2; exit 2; }

# Заглушка: $1 — имя файла, $2 — код возврата для `check`, $3 — для `emit`.
mk_novac() {
    cat > "$T/novac" <<STUB
#!/bin/sh
case "\$1" in
  check) exit $1 ;;
  emit)  exit $2 ;;
  *)     exit 99 ;;
esac
STUB
    chmod +x "$T/novac"
}
mk_oracle() {   # $1 — код возврата для `build`
    cat > "$T/oracle" <<STUB
#!/bin/sh
exit $1
STUB
    chmod +x "$T/oracle"
}

# Достаём из инструмента только функцию классификации, вместе с её окружением.
# Читать её из файла, а не переписывать здесь, — обязательное условие: копия
# разошлась бы с оригиналом на первой же правке (правило одного дома).
extract_classify() {
    awk '/^classify_emit_red\(\)/,/^}/' "$TOOL"
}

run_case() {   # $1 имя, $2 check-rc, $3 emit-rc, $4 oracle-rc, $5 ожидаемая корзина
    name="$1"; crc="$2"; erc="$3"; orc="$4"; want="$5"
    mk_novac "$crc" "$erc"
    if [ "$orc" = "none" ]; then rm -f "$T/oracle"; else mk_oracle "$orc"; fi
    mkdir -p "$T/cases"; : > "$T/cases/x.nv"
    got="$(
        NOVAC="$T/novac"
        ORACLE="$T/oracle"
        ROOT="$ROOT"
        T="$T"
        novac_is_panic_rc() { [ "$1" -ge 128 ] || [ "$1" -eq 101 ]; }
        eval "$(extract_classify)"
        classify_emit_red x.nv
    )"
    if [ "$got" = "$want" ]; then
        echo "  ok: $name -> $got"
    else
        echo "  FAIL: $name -> получено '$got', ожидалось '$want'" >&2
        fails=$((fails+1))
    fi
}

echo "test-novac-fuzz-emit-buckets: классификация красного случая"

# ХУДШИЙ класс: чекер принял, эмиттер упал, ОРАКУЛ СОБРАЛ. Форма законна.
run_case "check ok, emit panic, oracle built"   0 101 0    "LEGAL/NOVAC"
# Чекер принял то, что не должен: оракул ту же форму отверг.
run_case "check ok, emit panic, oracle refused" 0 101 1    "CHECKER"
# Чекер отказал — в живом конвейере до эмиссии не дошло бы.
run_case "check refused"                        1 101 0    "NOT-OUR-CASE"
# Падает и сам check — этот случай ловит судья уровнем выше.
run_case "check panics too"                     101 101 0  "CHECK-TOO"
# ОБРАТНАЯ СТОРОНА: нет оракула — случай НЕ РАЗОБРАН, а не «зелено».
run_case "oracle absent"                        0 101 none "ORACLE-ABSENT"

# --- ОБХОД, а не только классификация -------------------------------------
# ЗАЧЕМ ОТДЕЛЬНО: классификация решает, КАК назвать красный случай, а обход —
# БУДЕТ ли он вообще. Ошибка в обходе красит чужой ярус зря (ложная тревога)
# либо молчит на настоящей панике (пропуск). Обе стороны проверяются здесь.
extract_judge() {
    awk '/^judge_emit\(\)/,/^}/' "$TOOL"
}

run_judge() {   # $1 имя, $2 emit-rc заглушки, $3 ожидаемый код (0 зелено / 1 красно)
    name="$1"; erc="$2"; want="$3"
    mk_novac 0 "$erc"
    mkdir -p "$T/cases"; : > "$T/cases/a.nv"; : > "$T/cases/b.nv"
    printf "a.nv\nb.nv\n" > "$T/list.one"
    got=$(
        NOVAC="$T/novac"
        ROOT="$ROOT"
        T="$T"
        novac_is_panic_rc() { [ "$1" -ge 128 ] || [ "$1" -eq 101 ]; }
        eval "$(extract_judge)"
        judge_emit "$T/list.one"; echo $?
    )
    if [ "$got" = "$want" ]; then
        echo "  ok: обход, $name -> rc=$got"
    else
        echo "  FAIL: обход, $name -> rc=$got, ожидалось $want" >&2
        fails=$((fails+1))
    fi
}

# Зелёная сторона: эмиссия отработала — обход обязан вернуть 0 и НЕ красить ярус.
run_judge "эмиссия чистая" 0 0
# Красная сторона: паника (rc 101) — обход обязан её увидеть.
run_judge "эмиссия паникует" 101 1

if [ "$fails" -eq 0 ]; then
    echo "test-novac-fuzz-emit-buckets ok: 7 случаев — 5 корзин и 2 стороны обхода"
    exit 0
fi
echo "test-novac-fuzz-emit-buckets: провалов $fails" >&2
exit 1
