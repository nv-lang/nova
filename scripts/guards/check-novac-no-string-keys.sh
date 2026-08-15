#!/bin/sh
# scripts/guards/check-novac-no-string-keys.sh — идентичность — не имя:
# строковый ключ таблицы в novac/src вне закона.
# План: docs/plans/274-novac-self-hosted-compiler.md §10.3; архитектура — docs/dev/novac-architecture.md §4а.
#
# ПРАВИЛО (архитектура novac §4а; К2 §16, инварианты (а) и (б); страж назван
# в плане 274 §10.3): имя — ключ ровно в одной двери (`names`). ПОСЛЕ `names`
# ни одна таблица не ключуется строкой; ВНУТРИ `names` ключ несёт
# NamespaceId-компонент — законные формы `Map[(NamespaceId, ...)]` или
# `Map[NsKey, ...]`.
#
# Проверяет грепом по .nv-файлам novac/src:
#   * вне модуля names/ — любой 'Map[str' красный ('Map[str,', 'Map[string',
#     'HashMap[str' — все три содержат эту подстроку);
#   * внутри names/ — 'Map[str' красный, если на той же строке нет
#     'NamespaceId' (NamespaceId на строке считается компонентом ключа;
#     покрывает и вложенную форму Map[NamespaceId, Map[str, ...]]).
#
# НЕ проверяет: многострочные типы (законная форма пишется одной строкой —
# NamespaceId на другой строке даст красный, и это намеренно); подстроку
# судит и в комментариях/строковых литералах (строже, не мягче);
# содержательность таблиц — на приёмке. Нет novac/src или нет .nv-файлов —
# зелёный «судить нечего»: страж до кода легален, молчание нелегально (№645).
#
# $1 — корень репозитория (по умолчанию — вычислить от себя);
# $2 — override сканируемой директории (для самотеста; вместо novac/src).
#
# Проверялся: Windows (Git Bash), 2026-08-14.
export LC_ALL=C
ROOT="${1:-$(cd "$(dirname "$0")/../.." && pwd)}"
SRC="${2:-$ROOT/novac/src}"

if [ ! -d "$SRC" ]; then
    echo "check-novac-no-string-keys ok: судить нечего (нет $SRC, файлов .nv: 0)"
    exit 0
fi

NFILES=$(find "$SRC" -type f -name '*.nv' | wc -l | tr -d '[:space:]')
if [ "$NFILES" -eq 0 ]; then
    echo "check-novac-no-string-keys ok: судить нечего (в $SRC файлов .nv: 0)"
    exit 0
fi

BAD=$(find "$SRC" -type f -name '*.nv' | sort | while IFS= read -r f; do
    rel=${f#"$SRC"/}
    case "$rel" in
        names/*|*/names/*)
            grep -Fn 'Map[str' "$f" | grep -Fv 'NamespaceId' | sed "s|^|  $rel:|"
            ;;
        *)
            grep -Fn 'Map[str' "$f" | sed "s|^|  $rel:|"
            ;;
    esac
done)

# Вторая половина правила (владелец 2026-08-16): СИНТЕЗ ключа строкой.
# Дверь `names` легальна, поэтому первая половина молчала, когда ключ
# склеивали интерполяцией: `@names.put("${owner}.${fd.name}", row)` —
# аллокация и форматирование на КАЖДЫЙ поиск (П14) и структура, загнанная
# обратно в текст сразу после правила «идентичность — не имя» (§4а).
# Законная форма — цепочка одноимённых строк плюс целочисленное сравнение
# (см. FnTable/FieldTable в sem). Судим ровно вызовы двери с интерполяцией
# в первом аргументе; голое имя-переменная законно.
SYNTH=$(find "$SRC" -type f -name '*.nv' | sort | while IFS= read -r f; do
    rel=${f#"$SRC"/}
    grep -n '\.\(put\|find\)("[^"]*\${' "$f" | sed "s|^|  $rel:|"
done)

if [ -n "$SYNTH" ]; then
    echo "check-novac-no-string-keys: FAIL — ключ двери СИНТЕЗИРОВАН строкой (архитектура §4а, П17):" >&2
    printf '%s
' "$SYNTH" >&2
    echo "  Составной ключ из интерполяции стоит аллокации на каждый поиск (П14)" >&2
    echo "  и прячет структуру в текст. Законная форма: дверь берёт ИМЯ, а строки" >&2
    echo "  с одинаковым именем связаны полем next; второй ключ сравнивается целым" >&2
    echo "  числом при обходе цепочки (образцы: FnTable.row_of, FieldTable.field_type)." >&2
    exit 1
fi

if [ -n "$BAD" ]; then
    echo "check-novac-no-string-keys: FAIL — строковый ключ таблицы (архитектура §4а, К2 §16):" >&2
    printf '%s\n' "$BAD" >&2
    echo "  Вне names/ таблицы ключуются DeclId/NodeId, не именем (инвариант (б))." >&2
    echo "  Внутри names/ ключ несёт NamespaceId: Map[(NamespaceId, str), ...]" >&2
    echo "  или Map[NsKey, ...] (инвариант (а) К2)." >&2
    exit 1
fi

echo "check-novac-no-string-keys ok: файлов .nv: $NFILES, строковых ключей вне закона: 0, синтезированных ключей: 0"
exit 0
