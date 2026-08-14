#!/bin/sh
# scripts/guards/check-novac-frontend-shape.sh — форма фронтенд-сигнатур novac:
# нет Result, позиции при результате.
#
# ПРАВИЛО (план 274 §4 п.1 «Result уходит из сигнатур» и п.4 «позиции хранятся
# везде»; архитектура docs/dev/novac-architecture.md §6; таблица стражей —
# план 274 §10.3): фронтенд novac — функции над неизменяемым входом,
# возвращающие «результат плюс диагностики». Не `parse() -> Result[Tree, E]`,
# а `parse() -> (Tree, [Diagnostic])`: разбор всегда что-то возвращает,
# ошибки — данные рядом, не альтернатива результату.
#
# ПРОВЕРЯЕТ (строго, красный = exit 1): в novac/src/{lex,parse,tree,syntax}/*.nv
# экспортированная однострочная сигнатура `export fn ... -> Result[` — красная.
# Законная форма — пара «(узел, диагностики)».
#
# ПРОВЕРЯЕТ (МЯГКО — ТОЛЬКО ПРЕДУПРЕЖДЕНИЕ В STDOUT, НЕ exit 1; честно:
# эта часть гейт НЕ роняет): каждая экспортированная fn с параметром-текстом
# (тип `str` в параметрах) обязана упоминать Span/позиции в возврате
# (Span/Pos/Loc/Line/Col) ИЛИ файл обязан содержать тип с полем `span`.
# Нарушение печатается строкой WARN в stdout — сигнал приёмке, не блок.
#
# НЕ ПРОВЕРЯЕТ: многострочные сигнатуры (судятся только однострочные);
# неэкспортированные хелперы (им Result законен); содержательность диагностик;
# дисциплину отравления (свой страж); backend-модули вне lex/parse/tree/syntax.
#
# Аргумент $1 — корень репозитория (по умолчанию — вычислить от себя);
# $2 — override сканируемой директории вместо novac/src (для самотеста).
#
# Если судить нечего (нет novac/src или во фронтенд-модулях нет .nv) — зелёный
# с честной строкой «судить нечего»: страж до кода легален, молчание нелегально.
#
# Проверялся: Windows (Git Bash), 2026-08-14.
export LC_ALL=C
ROOT="${1:-$(cd "$(dirname "$0")/../.." && pwd)}"
SRC="${2:-$ROOT/novac/src}"

FILES=""
for d in lex parse tree syntax; do
    [ -d "$SRC/$d" ] || continue
    for f in "$SRC/$d"/*.nv; do
        [ -f "$f" ] || continue
        FILES="$FILES$f
"
    done
done

if [ -z "$FILES" ]; then
    echo "check-novac-frontend-shape ok: судить нечего (нет .nv во фронтенд-модулях novac/src/{lex,parse,tree,syntax}; файлов 0)"
    exit 0
fi

BAD=""
WARNS=""
nfiles=0
nexports=0
while IFS= read -r f; do
    [ -n "$f" ] || continue
    nfiles=$((nfiles+1))
    c=$(grep -cE '^[[:space:]]*export[[:space:]]+fn[[:space:]]' "$f")
    nexports=$((nexports+c))

    # --- строгая часть: export fn ... -> Result[ --------------------------
    hits=$(grep -nE '^[[:space:]]*export[[:space:]]+fn[[:space:]].*->[[:space:]]*Result[[:space:]]*\[' "$f")
    if [ -n "$hits" ]; then
        BAD="$BAD$(printf '%s\n' "$hits" | sed -e "s|^|  $f:|")
"
    fi

    # --- мягкая часть: параметр-текст без Span/позиций в возврате ---------
    # Файл с типом, несущим поле span, освобождает свои экспорты целиком.
    if ! grep -qE '^[[:space:]]*span[[:space:]]' "$f"; then
        w=$(awk -v FILE="$f" '
            /^[[:space:]]*export[[:space:]]+fn[[:space:]]/ {
                line=$0
                p=index(line,"(")
                a=index(line,"->")
                if (p==0) next
                if (a>0) { params=substr(line,p,a-p); ret=substr(line,a) }
                else     { params=substr(line,p);     ret="" }
                if (params ~ /(^|[^A-Za-z0-9_])str([^A-Za-z0-9_]|$)/) {
                    if (ret !~ /Span|Pos|Loc|Line|Col/)
                        printf "  %s:%d: %s\n", FILE, FNR, line
                }
            }
        ' "$f")
        if [ -n "$w" ]; then
            WARNS="$WARNS$w
"
        fi
    fi
done <<EOF
$FILES
EOF

if [ -n "$WARNS" ]; then
    echo "check-novac-frontend-shape WARN (мягко, гейт НЕ роняет): экспорт с параметром-текстом не упоминает Span/позиции в возврате, и в файле нет типа с полем span (план 274 §4 п.4 — позиции хранятся везде):"
    printf '%s' "$WARNS"
fi

if [ -n "$BAD" ]; then
    echo "check-novac-frontend-shape: FAIL — Result в экспортированных сигнатурах фронтенда:" >&2
    printf '%s' "$BAD" >&2
    echo "  Фронтенд не возвращает Result: форма — пара «(узел, диагностики)»," >&2
    echo "  ошибки — данные рядом с результатом, не альтернатива ему." >&2
    echo "  План 274 §4 п.1; архитектура §6." >&2
    exit 1
fi

echo "check-novac-frontend-shape ok: файлов $nfiles, экспортов fn $nexports, '-> Result[' во фронтенде: 0"
exit 0
