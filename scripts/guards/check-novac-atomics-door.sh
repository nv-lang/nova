#!/bin/sh
# scripts/guards/check-novac-atomics-door.sh — атомики и TLS только через
# одну дверь.
#
# ПРАВИЛО (план 274 §8.1; архитектура novac §10): всё атомарное и весь
# thread-local в novac/src идёт через ЕДИНСТВЕННЫЙ модуль src/atomics/ —
# дверь. Вне двери запрещены подстроки '__atomic_' и 'thread_local' (прямая
# работа с примитивами C) и 'nova_atomic_' (вызов рантайма мимо обёртки).
# Внутри src/atomics/ те же подстроки законны — дверь их и реализует.
#
# ЧТО ПРОВЕРЯЕТ: подстроки в файлах под novac/src вне */atomics/* (грепом,
# включая строковые литералы codegen — эмиссия атомика мимо двери тоже дверь
# в обход).
# ЧЕГО НЕ ПРОВЕРЯЕТ (сказано честно): семантику — гонку через обычную
# переменную страж не увидит; атомики вне novac/src (рантайм на C — другой
# периметр); полноту самой двери.
#
# Аргумент $1 — корень репозитория (по умолчанию — вычислить от себя);
# $2 — override сканируемой директории (для самотеста; default $ROOT/novac/src).
#
# Проверялся: Windows (Git Bash), 2026-08-14.
export LC_ALL=C
# Корень приводится к АБСОЛЮТНОМУ пути: относительный `.` уводил поиск
# бинаря мимо цели, и страж писал «сломан раннер» о здоровом дереве
# (2026-08-18). Ложная краснота стоит дороже отсутствующей проверки:
# по ней идут искать поломку, которой нет, и в стража перестают верить.
# Если cd не удался — значение СОХРАНЯЕТСЯ как было: пустой ROOT судил бы
# корень файловой системы, а это хуже исходной болезни.
ROOT="${1:-$(dirname "$0")/../..}"
ROOT="$(cd "$ROOT" 2>/dev/null && pwd || printf '%s' "$ROOT")"
SRC="${2:-$ROOT/novac/src}"

# Страж до кода легален: novac ещё может не существовать.
if [ ! -d "$SRC" ]; then
    echo "check-novac-atomics-door ok: судить нечего (нет $SRC)"
    exit 0
fi

N=0
BAD=""
while IFS= read -r f; do
    [ -n "$f" ] || continue
    N=$((N+1))
    hits=$(grep -n -e '__atomic_' -e 'thread_local' -e 'nova_atomic_' "$f" 2>/dev/null)
    if [ -n "$hits" ]; then
        BAD="$BAD$(printf '%s\n' "$hits" | sed "s|^|  $f:|")
"
    fi
done <<EOF
$(find "$SRC" -type f ! -path '*/atomics/*')
EOF

if [ -n "$BAD" ]; then
    echo "check-novac-atomics-door: FAIL — атомики/TLS мимо двери (274 §8.1, архитектура §10):" >&2
    printf '%s' "$BAD" >&2
    echo "  Чинить так: перенести примитив в novac/src/atomics/ и звать его" >&2
    echo "  обёртку оттуда; '__atomic_'/'thread_local'/'nova_atomic_' вне" >&2
    echo "  двери не живут — ни в коде, ни в эмитируемых строках." >&2
    exit 1
fi

if [ "$N" -eq 0 ]; then
    echo "check-novac-atomics-door ok: судить нечего (вне atomics/ нет файлов в $SRC)"
    exit 0
fi

echo "check-novac-atomics-door ok: файлов вне двери $N, атомики/TLS только через atomics/"
exit 0
