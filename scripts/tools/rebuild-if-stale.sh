#!/usr/bin/env bash
# scripts/tools/rebuild-if-stale.sh — пересобрать компилятор, если он устарел.
#
# ЗАЧЕМ. 2026-08-09 владелец потребовал показать `.c` для `str.bytes()`. Пример
# собрался СТАРЫМ бинарём: слияние `p172-sret` в 22:13, `nova.exe` от 17:08 —
# пять часов разницы. Исходник новый, бинарь старый, проверка молча шла против
# кода, которого в нём нет (реестр 221.1 №482). Интегратор до этого заявил
# владельцу результат, опираясь на отчёт окна, а не на свой прогон.
#
# РЕШЕНИЕ ВЛАДЕЛЬЦА: «после слияния должна происходить пересборка, можно слить
# несколько и затем пересобрать… нужен автомат на это».
#
# ПОЧЕМУ НЕ «ПЕРЕСОБИРАТЬ ВСЕГДА». Большинство слияний не трогают компилятор:
# планы, реестр, доки, скрипты. Пересборка там — чистая потеря минуты с лишним на
# каждое слияние, а лишняя цена дисциплины ведёт к её обходу. Поэтому решает не
# факт слияния, а ЗАТРОНУТЫЕ ПУТИ.
#
# ЧТО СЧИТАЕТСЯ «ТРОНУТ КОМПИЛЯТОР»: `compiler-codegen/**`, `nova-cli/**`,
# `Cargo.toml`/`Cargo.lock` в них. Рантайм `nova_rt/**` входит: он компилируется
# в состав, и его правки так же невидимы старому бинарю.
#
# ИСПОЛЬЗОВАНИЕ:
#   bash scripts/tools/rebuild-if-stale.sh            # пересобрать при нужде
#   bash scripts/tools/rebuild-if-stale.sh --check    # только сказать, устарел ли
#   bash scripts/tools/rebuild-if-stale.sh --since <ревизия>   # решать по диффу

set -u
export LC_ALL=C

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT" || exit 1

BIN="$ROOT/nova-cli/target/release/nova.exe"
[ -f "$BIN" ] || BIN="$ROOT/nova-cli/target/release/nova"

MODE=run
SINCE=""
while [ $# -gt 0 ]; do
    case "$1" in
        --check) MODE=check; shift ;;
        --since) SINCE="$2"; shift 2 ;;
        *) echo "rebuild-if-stale: неизвестный аргумент '$1'" >&2; exit 1 ;;
    esac
done

# ── Устарел ли бинарь ─────────────────────────────────────────────────────
# Сверка по ВРЕМЕНИ против последнего коммита, а не по флагу-маркеру: время
# нельзя забыть выставить, и оно верно даже если хук не стоял (новый клон,
# новая машина, чужой worktree).
stale_reason=""
if [ ! -f "$BIN" ]; then
    stale_reason="бинаря нет ($BIN)"
else
    B_TS=$(stat -c %Y "$BIN" 2>/dev/null)
    # Сверяем НЕ с HEAD, а с последним коммитом, ЗАТРОНУВШИМ компилятор:
    # иначе всякая правка планов и доков объявляла бы бинарь устаревшим, и
    # пересборка шла бы впустую — а лишняя цена дисциплины ведёт к её обходу.
    H_TS=$(git log -1 --format=%ct HEAD -- compiler-codegen nova-cli 2>/dev/null)
    if [ -n "${B_TS:-}" ] && [ -n "${H_TS:-}" ] && [ "$B_TS" -lt "$H_TS" ]; then
        stale_reason="бинарь ($(date -d "@$B_TS" '+%F %T' 2>/dev/null)) старше последней правки компилятора ($(date -d "@$H_TS" '+%F %T' 2>/dev/null))"
    fi
fi

# ── Тронут ли компилятор (когда спрашивают про конкретный дифф) ───────────
touched=1
if [ -n "$SINCE" ]; then
    if git diff --name-only "$SINCE" HEAD 2>/dev/null \
       | grep -qE '^(compiler-codegen|nova-cli)/'; then
        touched=1
    else
        touched=0
    fi
fi

if [ -z "$stale_reason" ]; then
    echo "rebuild-if-stale: бинарь свежий — пересборка не нужна"
    exit 0
fi

if [ "$touched" -eq 0 ]; then
    echo "rebuild-if-stale: $stale_reason, но компилятор слиянием НЕ тронут — пересборка не нужна"
    exit 0
fi

if [ "$MODE" = check ]; then
    echo "rebuild-if-stale: УСТАРЕЛ — $stale_reason" >&2
    exit 1
fi

echo "rebuild-if-stale: $stale_reason — пересобираю"
# ВАЖНО: собирать из `nova-cli`, а НЕ из корня — в корне нет `Cargo.toml`.
# Сам `cargo` при этом честно возвращает 101; ноль пришёл ОТ ОБЁРТКИ ФОНОВОГО
# ЗАПУСКА, сообщившей «exit code 0» на упавшей команде (№482, второй слой,
# диагноз исправлен 2026-08-09). Отсюда правило ниже: проверять АРТЕФАКТ, а не
# код возврата — код может быть чужим.
( cd "$ROOT/nova-cli" && cargo build --release ) || {
    echo "rebuild-if-stale: СБОРКА ПРОВАЛИЛАСЬ" >&2; exit 1; }

# Проверяем РЕЗУЛЬТАТ, а не код возврата: успешный код при несделанной работе —
# ровно то, на чём этот дефект и вскрылся.
if [ -f "$BIN" ]; then
    NEW_TS=$(stat -c %Y "$BIN" 2>/dev/null)
    H_TS=$(git log -1 --format=%ct HEAD -- compiler-codegen nova-cli 2>/dev/null)
    if [ -n "${NEW_TS:-}" ] && [ -n "${H_TS:-}" ] && [ "$NEW_TS" -lt "$H_TS" ]; then
        echo "rebuild-if-stale: сборка отчиталась успехом, но бинарь НЕ обновился" >&2
        exit 1
    fi
    echo "rebuild-if-stale: пересобрано, бинарь свежее HEAD"
    exit 0
fi
echo "rebuild-if-stale: после сборки бинаря нет ($BIN)" >&2
exit 1
