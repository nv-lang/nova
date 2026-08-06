#!/usr/bin/env bash
# install-guards.sh — УСТАНОВЩИК всех механизмов автопроверки (одной командой).
#
# ПОЧЕМУ (запрос владельца 2026-07-27: «должен быть файл установки автопроверок
# на случай, например, необходимости переустановки всего»). Часть механизмов
# живёт НЕ в файлах репы, а в НАСТРОЙКАХ окружения:
#   * git-хуки включаются `git config core.hooksPath` — ОТДЕЛЬНО в каждой из
#     пяти реп, и свежий `git clone` их НЕ приносит;
#   * хуки Claude Code читаются из `.claude/settings.json`.
# Пока эта установка держалась «в голове», её нельзя было ни воспроизвести на
# новой машине, ни проверить, ни восстановить после переустановки — то есть
# защита существовала ровно до первого чистого клона. Этот скрипт делает
# установку воспроизводимой и проверяемой.
#
# ЧТО ДЕЛАЕТ (идемпотентно — можно запускать сколько угодно раз):
#   1. `core.hooksPath` → `scripts/githooks` во ВСЕХ найденных репах семьи
#      (nova, nova-http, nova-tls, nova-polaris, nova-compress). Отсутствующие
#      рядом репы просто пропускаются с отметкой.
#   2. Права на исполнение всем стражам, самотестам и хукам.
#   3. Проверка, что хуки Claude Code объявлены в `.claude/settings.json`
#      (сам файл НЕ переписывается — он может нести и другие настройки;
#      скрипт лишь сообщает, если объявления нет).
#   4. Диагностика: прогон мета-стража + всех самотестов — установка считается
#      успешной, только если механизмы реально работают, а не просто «файлы на
#      месте» (правило владельца: «в скрипте нет толку, если не подключён к
#      автопроверке или сам содержит ошибки»).
#
# ЧЕГО НЕ ДЕЛАЕТ: не ставит компилятор/зависимости (это `docs/promts/
# read-project.md`), не трогает содержимое настроек кроме `core.hooksPath`.
#
# ИСПОЛЬЗОВАНИЕ:
#   scripts/guards/install-guards.sh            # установить и проверить
#   scripts/guards/install-guards.sh --check    # только проверить, ничего не менять
# Коды: 0 — всё установлено и работает; 1 — что-то не установилось/не прошло.
#
# План: docs/plans/231-bug-cycle-exit.md §4в (трек Ж).

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]:-$0}")" && pwd)"
# Скрипт живёт в scripts/guards/ — корень репы на два уровня выше.
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
FAMILY_PARENT="$(cd "$REPO_ROOT/.." && pwd)"
CHECK_ONLY=0
[ "${1:-}" = "--check" ] && CHECK_ONLY=1

problems=0
say()  { echo "  $1"; }
bad()  { echo "  ✗ $1" >&2; problems=$((problems + 1)); }

echo "install-guards: установка механизмов автопроверки"
[ "$CHECK_ONLY" -eq 1 ] && echo "  (режим --check: только проверка, изменений не вносится)"

# ── 1. git-хуки во всех репах семьи ───────────────────────────────────────────
echo "[1/4] git-хуки (core.hooksPath)"
for repo in nova nova-http nova-tls nova-polaris nova-compress; do
    dir="$FAMILY_PARENT/$repo"
    if [ ! -d "$dir/.git" ] && [ ! -f "$dir/.git" ]; then
        say "— $repo: репы рядом нет, пропуск"
        continue
    fi
    current="$(git -C "$dir" config --get core.hooksPath 2>/dev/null || true)"
    if [ "$current" = "scripts/githooks" ]; then
        say "ok: $repo — уже настроено"
        continue
    fi
    if [ "$CHECK_ONLY" -eq 1 ]; then
        bad "$repo: core.hooksPath = '${current:-<не задан>}' (ожидается scripts/githooks)"
        continue
    fi
    if [ ! -d "$dir/scripts/githooks" ]; then
        bad "$repo: нет каталога scripts/githooks — хуки не из чего ставить"
        continue
    fi
    if git -C "$dir" config core.hooksPath scripts/githooks 2>/dev/null; then
        say "установлено: $repo"
    else
        bad "$repo: не удалось задать core.hooksPath"
    fi
done

# ── 2. Права на исполнение ────────────────────────────────────────────────────
echo "[2/4] права на исполнение"
if [ "$CHECK_ONLY" -eq 1 ]; then
    say "пропуск (режим проверки)"
else
    chmod +x "$REPO_ROOT"/scripts/*.sh 2>/dev/null || true
    chmod +x "$REPO_ROOT"/scripts/guards/*.sh 2>/dev/null || true
    chmod +x "$REPO_ROOT"/scripts/guards/selftest/*.sh 2>/dev/null || true
    chmod +x "$REPO_ROOT"/scripts/tools/*.sh 2>/dev/null || true
    chmod +x "$REPO_ROOT"/scripts/githooks/* 2>/dev/null || true
    chmod +x "$REPO_ROOT"/scripts/claude-hooks/*.py 2>/dev/null || true
    say "выставлены (scripts, guards, guards/selftest, tools, githooks, claude-hooks)"
fi

# ── 3. Хуки Claude Code объявлены в настройках ────────────────────────────────
echo "[3/4] хуки Claude Code (.claude/settings.json)"
settings="$REPO_ROOT/.claude/settings.json"
if [ ! -f "$settings" ]; then
    bad "нет $settings — хуки агентов не подключены"
else
    for hook in guard-git guard-memory; do
        if grep -q "$hook" "$settings"; then
            say "ok: $hook объявлен"
        else
            bad "$hook НЕ объявлен в $settings (файл не переписываю — допиши вручную)"
        fi
    done
fi

# ── 4. Диагностика: механизмы должны РАБОТАТЬ, а не просто лежать ────────────
echo "[4/4] диагностика — прогон мета-стража и самотестов"
if bash "$REPO_ROOT/scripts/guards/check-guard-wiring.sh" >/dev/null 2>&1; then
    say "ok: все стражи документированы, подключены, покрыты самотестами"
else
    bad "мета-страж не прошёл — запусти scripts/guards/check-guard-wiring.sh и почини"
fi
shopt -s nullglob
for st in "$REPO_ROOT"/scripts/guards/selftest/test-*.sh; do
    if bash "$st" >/dev/null 2>&1; then
        say "ok: самотест $(basename "$st")"
    else
        bad "самотест ПРОВАЛЕН: $(basename "$st")"
    fi
done

echo
if [ "$problems" -ne 0 ]; then
    echo "install-guards: НЕ ЗАВЕРШЕНО — проблем: $problems" >&2
    exit 1
fi
echo "install-guards ok: механизмы установлены и проверены"
