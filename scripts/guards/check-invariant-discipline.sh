#!/usr/bin/env bash
# scripts/guards/check-invariant-discipline.sh
#
# ЭНФОРС НОРМЫ ОБ ИНВАРИАНТАХ — `docs/dev/conventions-governance.md`,
# раздел «Инварианты: как можно меньше, и ни одного на честном слове».
#
# ТРЕБОВАНИЕ ВЛАДЕЛЬЦА 2026-08-08 (дословно): «требования по инвариантам обязаны
# быть всеобъемлющими для любых работ по языку Nova, компилятору, nova-cli, lsp,
# модулям и пакетам — всё, что мы делаем в этом проекте, должно подчиняться этим
# требованиям, и автоматически проверяться, НЕ ВАЖНО, ПОМНИШЬ ТЫ ИЛИ ДРУГОЙ
# АГЕНТ ОБ ЭТОМ».
#
# МНОГОРЕПНОСТЬ. Пакеты живут в отдельных репозиториях (`nova-tls`, `nova-http`,
# `nova-polaris`, `nova-compress`, `nova-socks`, `nova-bignum`), и гейт `nova`
# их не видит. Поэтому страж ПЕРЕНОСИМ: принимает корень аргументом и не зависит
# от дерева `nova`. Раздаётся `scripts/tools/sync-guards-to-packages.sh`,
# расхождение копий ловит гейт `nova`.
#
# ЧТО ЛОВИТ. Инвариант, держащийся договорённостью, имеет узнаваемую ПОДПИСЬ —
# комментарий, который сам признаётся, что правило держится на честном слове:
# «взаимоисключающие», «parser enforce'ит», «обязательно вызывать перед»,
# «mutually exclusive», «must be called before». Страж ищет такие фразы в
# ДОБАВЛЯЕМЫХ строках и требует рядом пометку об уровне энфорса.
#
# ПОЧЕМУ ИМЕННО ТАК. Проверить «нет ли нового инварианта» машинно нельзя — это
# суждение. Но КАЖДЫЙ класс дефектов, найденный 2026-08-08, имел в коде именно
# такой комментарий: №462 («взаимоисключающие, parser enforce'ит», 68 мест),
# №447 (соглашение на 4 сайтах), №459, №453. Ловим подпись — ловим класс.
#
# КАК ПРОЙТИ ПРОВЕРКУ. Рядом со строкой поставить ОДНУ из пометок:
#   [INV-PROPERTY]            — уже не инвариант: нарушение НЕВЫРАЗИМО (шаг 0/1).
#   [INV-GUARD: <имя-стража>] — держится машинно; страж обязан существовать
#                               в scripts/guards/ и иметь селфтест.
#   [INV-TODO: №NNN]          — признанный долг, заведён в реестре 221.1.
#
# ЧТО ПРОВЕРЯЕТСЯ. Только то, что КОММИТИТСЯ: дифф против базы плюс индекс.
# Неотслеживаемый мусор рабочего дерева не сканируется — первая редакция тянула
# 175 посторонних файлов и не укладывалась в таймаут.
#
# ИСПОЛЬЗОВАНИЕ:
#   bash scripts/guards/check-invariant-discipline.sh [КОРЕНЬ] [БАЗА]

set -u
export LC_ALL=C

ROOT="${1:-$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)}"
BASE="${2:-origin/main}"
cd "$ROOT" || exit 1

VIOL=0

# ── Сбор изменений ────────────────────────────────────────────────────────
DIFF=""
if git rev-parse --verify --quiet "$BASE" >/dev/null 2>&1; then
    DIFF=$(git diff "$BASE"...HEAD --unified=0 2>/dev/null)
fi
DIFF="$DIFF
$(git diff --unified=0 2>/dev/null)
$(git diff --cached --unified=0 2>/dev/null)"

# ── Один проход awk ───────────────────────────────────────────────────────
# Построчный цикл с двумя вызовами grep на строку давал тысячи процессов на
# 11 000 строк диффа и не укладывался в 300 с. Страж, который не успевает
# отработать, — не страж (тот же урок, что у check-no-path-deps).
OUT=$(printf '%s\n' "$DIFF" | awk '
    BEGIN {
        sig  = "взаимоисключающ|mutually exclusive|parser enforce|обязательно вызывать"
        sig2 = "must be called before|держится соглашением|по договорённости|by convention only"
        mark = "INV-PROPERTY|INV-GUARD:|INV-TODO:"
    }
    /^\+\+\+ b\// { f = substr($0, 7); next }
    /^\+/ {
        if (f == "") next
        if (f ~ /^docs\// || f ~ /^spec\// || f ~ /\.md$/) next
        if (f ~ /libuv\// || f ~ /minicoro\.h$/) next
        if (f !~ /\.(rs|c|h|nv|sh|py)$/) next
        body = substr($0, 2)
        if (body !~ sig && body !~ sig2) next
        if (body ~ mark) next
        print "check-invariant-discipline: НАРУШЕНИЕ — " f
        print "    " body
    }
')

if [ -n "$OUT" ]; then
    echo "$OUT" >&2
    echo "    ^ инвариант на честном слове. Поставь рядом [INV-PROPERTY]," >&2
    echo "      [INV-GUARD: <страж>] либо [INV-TODO: №NNN]." >&2
    echo "      Норма: docs/dev/conventions-governance.md, раздел «Инварианты»." >&2
    VIOL=$(printf '%s\n' "$OUT" | grep -c 'НАРУШЕНИЕ')
fi

# ── Каждая ссылка на стража обязана указывать на существующего стража с селфтестом ──
# `--exclude-dir=selftest`: в селфтестах такие строки лежат как ТЕСТОВЫЕ ДАННЫЕ
# (проверяем, что страж ловит ссылку на несуществующего стража) — сканировать их
# значит ловить самого себя. Поймано этим же стражем на себе.
for g in $(grep -rhoE '\[INV-GUARD: *[a-zA-Z0-9._-]+' --exclude-dir=selftest \
             --include=*.rs --include=*.c --include=*.h --include=*.nv --include=*.sh \
             . 2>/dev/null | sed 's/.*\[INV-GUARD: *//' | sort -u); do
    if [ ! -f "scripts/guards/${g}" ] && [ ! -f "scripts/guards/${g}.sh" ]; then
        echo "check-invariant-discipline: НАРУШЕНИЕ — [INV-GUARD: $g] указывает на несуществующего стража" >&2
        VIOL=$((VIOL + 1))
        continue
    fi
    base="${g%.sh}"
    if [ ! -f "scripts/guards/selftest/test-${base}.sh" ]; then
        echo "check-invariant-discipline: НАРУШЕНИЕ — у стража $g НЕТ селфтеста" >&2
        echo "    страж без селфтеста не работает (урок LC_ALL=C и measure.sh)" >&2
        VIOL=$((VIOL + 1))
    fi
done

if [ "$VIOL" -gt 0 ]; then
    echo "check-invariant-discipline: FAIL — $VIOL нарушени(й) нормы об инвариантах" >&2
    exit 1
fi
echo "check-invariant-discipline ok: новых инвариантов на честном слове нет"
exit 0
