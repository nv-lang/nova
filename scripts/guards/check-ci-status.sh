#!/usr/bin/env bash
# check-ci-status.sh — страж внешнего авторитетного гейта (GitHub Actions).
#
# ЗАЧЕМ (реестр 221.1 №395/№401/№402; правило владельца 2026-08-07:
# «проверка и слежение должны запускаться автоматически, а не потому что
# кто-то помнит»).
#
# Прецедент, ради которого страж заведён — сутки 2026-08-05/06:
#   * CI на github был красным по ЧЕТЫРЁМ воркфлоу, и за 24 слияния подряд
#     никто в него не заглянул: локальный `gate.sh` был зелёным, и этого
#     казалось достаточно. Обнаружено только прямым вопросом владельца.
#   * Хуже: на пуш `7ed38407d` не стартовало НИ ОДНОГО прогона при включённых
#     Actions и корректных триггерах. Не запустившийся гейт опаснее красного —
#     красный виден, молчащий выглядит как «всё хорошо».
#   * Первопричина расхождения структурная: локальный гейт гоняет
#     `nova check std/src`, а CI — `nova test std`. То есть CI видит то, чего
#     локальный гейт не видит ПО ПОСТРОЕНИЮ (№402: `retry_test` не собирается
#     при зелёном `check`). Пока это так, «локально зелено» — более слабое
#     утверждение, чем звучит, и об этом обязана напоминать машина.
#
# ПЕРЕД ТЕМ КАК КОПАТЬ ПО ВЕРДИКТУ `STALE` — ПОСМОТРИ НАРУЖУ:
#   https://www.githubstatus.com/api/v2/summary.json
# Именно так закончился первый случай, ради которого написан этот страж: 2026-08-06
# прогоны перестали запускаться по push, было перебрано полдюжины внутренних
# гипотез (настройки организации, состояние воркфлоу, skip-маркеры, права,
# биллинг), а причиной оказался ГЛОБАЛЬНЫЙ СБОЙ GitHub Actions — Major Outage с
# 15:22 UTC, инцидент дословно: «многие события push и pull request не запускают
# новые workflow runs». Признак внешнего сбоя: то же самое видно в ДРУГИХ репах
# семьи, а ручной `workflow_dispatch` при этом проходит.
#
# ЧТО ДЕЛАЕТ: спрашивает GitHub про прогоны на текущем `origin/main` (или на
# хеше из аргумента) и печатает вердикт рядом с локальным. Три исхода:
#   OK    — на хеше есть прогоны и все завершившиеся зелёные;
#   RED   — есть хотя бы один `failure`/`cancelled`/`timed_out`;
#   STALE — прогонов на хеше НЕТ вовсе, а с момента коммита прошло больше
#           NOVA_CI_STALE_MIN минут (по умолчанию 20). Это и есть случай
#           «пуш прошёл, CI не отреагировал».
#
# РЕЖИМ ПО УМОЛЧАНИЮ — НЕ БЛОКИРУЮЩИЙ (exit 0 при любом вердикте): страж
# сообщает, а не останавливает работу, потому что внешний сервис бывает
# недоступен, и падение сети не должно ронять локальный гейт. Блокирующий
# режим включается флагом `--strict` (используется в pre-push хуке): там
# красный внешний гейт ОБЯЗАН остановить отправку.
#
# `gh` отсутствует / не авторизован / сеть недоступна → вердикт SKIP, exit 0,
# с явной строкой о причине. Молчания быть не должно ни в одном случае: страж,
# который ничего не напечатал, неотличим от стража, который всё проверил.
#
# ИСПОЛЬЗОВАНИЕ:
#   check-ci-status.sh [--strict] [хеш]
#
# LC_ALL=C — байтовый grep независимо от локали хоста (msys2-grep в
# ru_RU.UTF-8 на не-ASCII молча даёт ноль хитов; см. reference по стражам).
export LC_ALL=C

set -u

STRICT=0
SHA_ARG=""
for arg in "$@"; do
    case "$arg" in
        --strict) STRICT=1 ;;
        *)        SHA_ARG="$arg" ;;
    esac
done

STALE_MIN="${NOVA_CI_STALE_MIN:-20}"
REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]:-$0}")/../.." && pwd)"
cd "$REPO_ROOT" || exit 0

say()  { echo "check-ci-status: $*"; }
skip() { say "SKIP — $*"; exit 0; }

command -v gh >/dev/null 2>&1 || skip "gh не установлен (внешний гейт не проверен)"
gh auth status >/dev/null 2>&1 || skip "gh не авторизован (внешний гейт не проверен)"

if [ -n "$SHA_ARG" ]; then
    SHA="$SHA_ARG"
else
    git fetch github main --quiet 2>/dev/null || git fetch origin main --quiet 2>/dev/null || true
    SHA="$(git rev-parse github/main 2>/dev/null || git rev-parse origin/main 2>/dev/null || true)"
fi
[ -n "$SHA" ] || skip "не удалось определить хеш удалённого main"

SHORT="$(echo "$SHA" | cut -c1-9)"

RUNS="$(gh run list --limit 40 \
        --json name,status,conclusion,headSha,createdAt 2>/dev/null || true)"
[ -n "$RUNS" ] || skip "gh не вернул список прогонов (сеть/лимит?)"

# Интерпретатор: в Ubuntu (и на CI) `python` ОТСУТСТВУЕТ — есть только
# `python3`. Без этого фолбэка страж на Linux молча уходил в SKIP и не
# проверял ничего (найдено прогоном самотестов в WSL 2026-08-07, тот же
# класс, что №404: инструмент писался и проверялся только на Windows).
# Выбираем по ФАКТУ ИСПОЛНЕНИЯ, а не по наличию в PATH: в Windows
# `python3` часто резолвится в заглушку Microsoft Store, которая существует,
# запускается и не делает НИЧЕГО — проверка `command -v` её пропускает, и
# страж молча уходил бы в SKIP на машине, где python есть.
PY_BIN=""
for _cand in python3 python; do
    if command -v "$_cand" >/dev/null 2>&1 && [ "$("$_cand" -c 'print(42)' 2>/dev/null)" = "42" ]; then
        PY_BIN="$_cand"; break
    fi
done
[ -n "$PY_BIN" ] || skip "не найден рабочий python/python3 (разбор ответа gh невозможен)"
VERDICT="$(printf '%s' "$RUNS" | "$PY_BIN" -c "
import json,sys
sha=sys.argv[1]
try:
    runs=json.load(sys.stdin)
except Exception:
    print('SKIP|не разобрать ответ gh'); raise SystemExit
mine=[r for r in runs if r.get('headSha','').startswith(sha[:9])]
if not mine:
    print('NONE|'); raise SystemExit
red=[r for r in mine if r.get('conclusion') in ('failure','cancelled','timed_out','startup_failure')]
run=[r for r in mine if r.get('status') != 'completed']
if red:
    print('RED|' + ', '.join(sorted({r['name'] for r in red})))
elif run:
    print('RUNNING|' + ', '.join(sorted({r['name'] for r in run})))
else:
    print('OK|' + str(len(mine)))
" "$SHA" 2>/dev/null)"

KIND="${VERDICT%%|*}"
DETAIL="${VERDICT#*|}"

case "$KIND" in
    OK)
        say "OK — внешний гейт зелёный на $SHORT ($DETAIL прогонов)"
        exit 0
        ;;
    RUNNING)
        say "идёт — на $SHORT ещё выполняются: $DETAIL"
        exit 0
        ;;
    RED)
        say "RED — ВНЕШНИЙ ГЕЙТ КРАСНЫЙ на $SHORT: $DETAIL"
        say "  локальный зелёный вердикт этого НЕ отменяет: gate.sh гоняет"
        say "  \`nova check std\`, CI — \`nova test std\` (реестр №402)."
        [ "$STRICT" -eq 1 ] && exit 1
        exit 0
        ;;
    NONE)
        CT="$(git show -s --format=%ct "$SHA" 2>/dev/null || echo 0)"
        NOW="$(date +%s)"
        AGE_MIN=$(( (NOW - CT) / 60 ))
        # `-ge`, а не `-gt`: с порогом 0 («считать молчащим сразу») строгое сравнение
        # никогда не срабатывает на свежем коммите — самотест на Linux падал именно
        # так (клон --depth 1, возраст HEAD = 0 мин, порог 0 → 0 > 0 = false).
        if [ "$CT" -gt 0 ] && [ "$AGE_MIN" -ge "$STALE_MIN" ] && [ "$STALE_MIN" -ge 0 ]; then
            say "STALE — на $SHORT НЕТ НИ ОДНОГО ПРОГОНА, коммиту $AGE_MIN мин"
            say "  пуш прошёл, CI не отреагировал — молчащий гейт опаснее красного"
            [ "$STRICT" -eq 1 ] && exit 1
            exit 0
        fi
        say "прогонов на $SHORT пока нет (коммиту $AGE_MIN мин, порог $STALE_MIN)"
        exit 0
        ;;
    SKIP)
        skip "$DETAIL"
        ;;
    *)
        skip "неожиданный вердикт разбора"
        ;;
esac
