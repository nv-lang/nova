#!/usr/bin/env bash
# scripts/guards/check-repo-root-clean.sh
# Корень репозитория — витрина, а не стол. Список файлов в нём закрытый.
#
# ДОМ И ОСНОВАНИЕ: реестр 221.1 №607; указание владельца 2026-08-12
# («что в репе делают файлы PROGRESS-… ? не надо этого делать»).
#
# ЗАЧЕМ. Окна писали чекпоинты прогресса (правило «сохраняй прогресс, обрыв
# должен стоить одного куска») прямо в корень: `PROGRESS-pchan.md`,
# `PROGRESS-pvela2.md` — шестнадцать штук. Каждый пришёл со своим коммитом, и
# ни один не убрали, потому что убирать было некому: окно закрылось, а файл
# остался. Владелец увидел их на первой странице публичного репозитория —
# то есть первое, что видит пришедший на язык, это наши рабочие записки.
#
# ЧТО ПРОВЕРЯЕТСЯ: среди ОТСЛЕЖИВАЕМЫХ файлов корня нет ничего, кроме
# перечисленного в белом списке ниже. Неотслеживаемое не трогаем: локальный
# черновик владельца — его дело, и страж в него не лезет.
#
# КУДА ВМЕСТО КОРНЯ: чекпоинты окон — `docs/plans/wip/`. Там они уже лежат
# десятками, и там их читает следующий, а не случайный гость.
#
# ИСПОЛЬЗОВАНИЕ:
#   bash scripts/guards/check-repo-root-clean.sh [КОРЕНЬ]
# Самотест — scripts/guards/selftest/test-check-repo-root-clean.sh

set -u
export LC_ALL=C

ROOT="${1:-$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)}"

# Белый список. Расширять ТОЛЬКО осознанно: каждая строка здесь — файл,
# который увидит первым любой пришедший.
ALLOWED='
.dockerignore
.editorconfig
.gitattributes
.gitignore
.gitmodules
AGENTS.md
CHANGELOG.md
CLAUDE.md
CONTRIBUTING.md
LICENSE
LICENSE-APACHE
LICENSE-MIT
NOTES.md
README.md
README.ru.md
SECURITY.md
bench.toml
nova.toml
'

FOUND=$(git -C "$ROOT" ls-files --full-name 2>/dev/null | grep -v '/')
if [ -z "$FOUND" ]; then
    echo "check-repo-root-clean ok: git не отдал списка файлов ($ROOT) — проверять нечего"
    exit 0
fi

BAD=""
for f in $FOUND; do
    case "$(printf '%s' "$ALLOWED" | grep -Fx "$f")" in
        "") BAD="$BAD $f" ;;
    esac
done

if [ -n "$BAD" ]; then
    echo "check-repo-root-clean: в корне лежит непредусмотренное:" >&2
    for f in $BAD; do echo "    $f" >&2; done
    echo "" >&2
    echo "    Корень — первая страница публичного репозитория. Рабочие" >&2
    echo "    записки окон живут в docs/plans/wip/ (реестр 221.1 №607)." >&2
    echo "    Если файл ДЕЙСТВИТЕЛЬНО должен встречать гостя — впиши его в" >&2
    echo "    белый список этого стража и скажи в коммите, почему." >&2
    echo "check-repo-root-clean: FAIL" >&2
    exit 1
fi

echo "check-repo-root-clean ok: в корне только предусмотренное"
exit 0
