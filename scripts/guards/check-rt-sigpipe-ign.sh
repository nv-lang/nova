#!/bin/sh
# scripts/guards/check-rt-sigpipe-ign.sh — SIG_IGN(SIGPIPE) обязан жить в
# двери драйвера.
#
# План/реестр: docs/plans/221.1-bug-sweep.md №664 (носитель №662) —
# vendored libuv на Linux пишет голым
# write(), и без SIG_IGN любая гонка «peer закрылся → мы пишем» убивает
# любой сетевой Nova-процесс сигналом 13 без core и без сообщения.
#
# ПРАВИЛО: тело nova_driver_init в compiler-codegen/nova_rt/driver.c
# содержит незакомментированный вызов signal(SIGPIPE, SIG_IGN). Дверь
# именно эта: runtime_init — явный тюнер, обычный бинарь его не зовёт
# (доказано мостом: фикс там смерть не снял).
#
# НЕ проверяет: поведение под нагрузкой (это петля №662-стенда) и
# Windows-ветку (там SIGPIPE нет, #ifndef _WIN32 — законен).
#
# $1 — корень репозитория (default: вычислить от себя).
#
# Проверялся: Windows (Git Bash), 2026-08-15.
export LC_ALL=C
# Корень приводится к АБСОЛЮТНОМУ пути: относительный `.` уводил поиск
# бинаря мимо цели, и страж писал «сломан раннер» о здоровом дереве
# (2026-08-18). Ложная краснота стоит дороже отсутствующей проверки:
# по ней идут искать поломку, которой нет, и в стража перестают верить.
# Если cd не удался — значение СОХРАНЯЕТСЯ как было: пустой ROOT судил бы
# корень файловой системы, а это хуже исходной болезни.
ROOT="${1:-$(dirname "$0")/../..}"
ROOT="$(cd "$ROOT" 2>/dev/null && pwd || printf '%s' "$ROOT")"
NAME=check-rt-sigpipe-ign

F="$ROOT/compiler-codegen/nova_rt/driver.c"
if [ ! -f "$F" ]; then
    echo "$NAME: FAIL — нет $F" >&2
    exit 1
fi

# Вырезаем тело nova_driver_init (до следующей функции на нулевом отступе)
# и ищем живой (не закомментированный построчно) вызов.
BODY=$(awk '/^void nova_driver_init\(void\)/{f=1} f{print} f && /^}/ && NR>1 && !/nova_driver_init/{exit}' "$F")
HIT=$(printf '%s\n' "$BODY" | grep -v '^[[:space:]]*\*' | grep -v '^[[:space:]]*/\*' \
      | grep -c 'signal(SIGPIPE, SIG_IGN)')

if [ "$HIT" -lt 1 ]; then
    echo "$NAME: FAIL — в теле nova_driver_init нет живого signal(SIGPIPE, SIG_IGN) (№664)" >&2
    echo "  Без него любой сетевой Nova-бинарь на Linux умирает молча от" >&2
    echo "  гонки «peer закрылся → мы пишем» (exit 141, без core)." >&2
    exit 1
fi
echo "$NAME ok: SIG_IGN(SIGPIPE) живёт в двери nova_driver_init (№664)"
exit 0
