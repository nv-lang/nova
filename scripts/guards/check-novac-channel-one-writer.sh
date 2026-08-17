#!/bin/sh
# scripts/guards/check-novac-channel-one-writer.sh — канал чекера пишет ОДИН
# чекер, а вывод типов ниже чекера не живёт (архитектура, раздел «Канал
# чекера»: «после чекера ни один потребитель не вызывает вывод типа»).
#
# ЗАЧЕМ. Архитектура называла этот страж прозой — «греп по novac/src/** вне
# novac/src/check/ даёт ноль вхождений unify, infer_, fresh_var», — но файла
# не было, и правило держалось на памяти автора. Оно ровно того класса, что
# план 196 снимал месяцами в нынешнем компиляторе: вторая дверь к выводу типа
# заводится не решением, а тем, что автор бэкенда не нашёл первую. До Э2-б1
# так и было: `emit_c` звал `sem.type_of` прямо во время эмиссии.
#
# ПРОВЕРЯЕТ по novac/src/**/*.nv (тесты *_test.nv тоже: тест, зовущий писателя
#   канала мимо чекера, — та же вторая дверь):
#   (A) писатели канала (`record_type`, `record_callee`, `record_subst`)
#       вызываются ТОЛЬКО из novac/src/check/ и определяются только в
#       novac/src/sem/channel.nv;
#   (B) вывод типа (`type_of(` как СВОБОДНЫЙ вызов решётки, `unify`, `infer_`,
#       `fresh_var`) не встречается вне novac/src/check/. Чтение канала
#       (`.type_of(` на приёмнике) — законно и не считается: это ЧТЕНИЕ
#       решения, а не вывод.
#   (C) сам файл канала существует и объявляет `CheckOut` — иначе страж судил
#       бы воздух (класс №519).
# НЕ ПРОВЕРЯЕТ: что записанное ВЕРНО (это дифф-корпус и байт-в-байт C);
#   полноту канала (тотальный обход — Э2-б3, и до него дыра ловится тотальным
#   читателем, который падает ice).
#
# $1 — корень репозитория; $2 — override сканируемой директории (шов самотеста).
# Проверялся: Windows (Git Bash), 2026-08-16.
export LC_ALL=C
ROOT="${1:-$(cd "$(dirname "$0")/../.." && pwd)}"
SRC="${2:-$ROOT/novac/src}"
NAME=check-novac-channel-one-writer

if [ ! -d "$SRC" ]; then
    echo "$NAME ok: судить нечего (нет $SRC)"
    exit 0
fi

T="${TMPDIR:-/tmp}/novac-channel-writer.$$"
mkdir -p "$T" || exit 1
trap 'rm -rf "$T"' 0

# --- (C) мишень на месте ---------------------------------------------------
CHAN=$(find "$SRC" -type f -name 'channel.nv' | head -n 1)
if [ -z "$CHAN" ] || ! grep -q 'export type CheckOut' "$CHAN" 2>/dev/null; then
    echo "$NAME: FAIL — не найден файл канала с 'export type CheckOut': страж потерял мишень (класс №519)" >&2
    exit 1
fi

BAD=""

# --- (A) писатели зовутся только из check ----------------------------------
for f in $(find "$SRC" -type f -name '*.nv' | sort); do
    rel=${f#"$SRC"/}
    case "$rel" in
        check/*) continue ;;      # единственный законный писатель
        sem/channel.nv) continue ;; # определение дверей — не вызов
    esac
    hits=$(grep -n 'record_type(\|record_callee(\|record_subst(' "$f" | grep -v '^\s*//' || true)
    if [ -n "$hits" ]; then
        BAD="$BAD
  $rel — зовёт писателя канала вне check/:
$(printf '%s' "$hits" | sed 's/^/      /')"
    fi
done

# --- (0) ПРЯМАЯ ЗАПИСЬ В ПОЛЕ КАНАЛА, мимо всякой двери ---------------------
# Инвариант «ONE writer» держался на том, что никто не пробовал написать
# напрямую: поля CheckOut публичны, и `out.types[id] = 0` из любого модуля
# компилируется. Адверсарная проверка 2026-08-17 это и сделала — проба
# собралась оракулом, страж остался зелёным, потому что грепал только имена
# дверей `record_*`. Инвариант, который держится на вежливости читателя, не
# инвариант; здесь судится сама форма записи.
#
# Законно: определение дверей в sem/channel.nv (там записи и живут) и
# конструирование канала целиком (`CheckOut { ... }`) — это не запись в
# чужую таблицу, а создание своей.
CHAN_FIELDS='types|callees|substs|subst_args'
for f in $(find "$SRC" -type f -name '*.nv' | sort); do
    rel=${f#"$SRC"/}
    case "$rel" in
        sem/channel.nv) continue ;;
    esac
    hits=$(grep -nE "\.($CHAN_FIELDS)\[[^]]*\][[:space:]]*=[^=]" "$f" | grep -v '^[0-9]*:[[:space:]]*//' || true)
    hits2=$(grep -nE "\.($CHAN_FIELDS)[[:space:]]*=[^=]" "$f" | grep -v '^[0-9]*:[[:space:]]*//' || true)
    both=$(printf '%s\n%s' "$hits" "$hits2" | grep -v '^$' || true)
    if [ -n "$both" ]; then
        BAD="$BAD
  $rel — ПРЯМАЯ запись в таблицу канала мимо двери:
$(printf '%s' "$both" | sed 's/^/      /')"
    fi
done


# --- (B) вывод типа не живёт ниже чекера -----------------------------------
for f in $(find "$SRC" -type f -name '*.nv' | sort); do
    rel=${f#"$SRC"/}
    case "$rel" in
        check/*) continue ;;
        sem/channel.nv) continue ;; # сам канал ОБЪЯВЛЯЕТ читателя type_of — это дверь чтения, а не вывод
    esac
    # `.type_of(` — чтение канала (законно); голый `type_of(` — вызов решётки.
    hits=$(tr -d '\r' < "$f" | grep -n 'unify(\|fresh_var(\|infer_[a-z_]*(\|[^.a-zA-Z_]type_of(' \
           | grep -v '^[0-9]*:[[:space:]]*//' | grep -v '^[0-9]*:[[:space:]]*///' || true)
    if [ -n "$hits" ]; then
        BAD="$BAD
  $rel — вывод типа вне check/ (вторая дверь к типу, класс плана 196):
$(printf '%s' "$hits" | sed 's/^/      /')"
    fi
done

if [ -n "$BAD" ]; then
    echo "$NAME: FAIL — у канала чекера появился второй писатель или вывод типа уехал ниже чекера:" >&2
    printf '%s\n' "$BAD" >&2
    echo "  Правило: пишет ТОЛЬКО check, остальные ЧИТАЮТ (out.type_of(id)). Нужен новый" >&2
    echo "  факт о типе — его записывает чекер, а не вычисляет потребитель." >&2
    exit 1
fi

NF=$(find "$SRC" -type f -name '*.nv' | wc -l | tr -d '[:space:]')
NW=$(grep -c 'record_type(\|record_callee(\|record_subst(' "$SRC"/check/*.nv 2>/dev/null | awk -F: '{s+=$NF} END {print s+0}')
echo "$NAME ok: файлов .nv: $NF, вызовов писателей канала: $NW (все в check/), вывода типа вне чекера: 0"
exit 0
