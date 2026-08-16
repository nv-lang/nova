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
# ЧТО ПРОВЕРЯЕТСЯ: среди ОТСЛЕЖИВАЕМЫХ файлов И КАТАЛОГОВ корня нет ничего,
# кроме перечисленного в двух белых списках ниже. Неотслеживаемое не трогаем:
# локальный черновик владельца — его дело, и страж в него не лезет.
#
# КАТАЛОГИ — с 2026-08-16 (реестр №695). До этого страж брал `ls-files | grep
# -v '/'` — то есть видел только файлы, а каталоги были ему невидимы вовсе.
# Через эту дыру в корень прошли и ПРОЖИЛИ там неделями пять черновиков окон:
# scratch259/, scratch457/, scratch465/, probes-p383/, .p259/ — при том что
# страж на каждом прогоне печатал «в корне только предусмотренное». Улики,
# на которые ссылаются реестр/спека/план, живут в docs/plans/repro/ (README
# там); черновики окна в репозиторий не попадают вовсе.
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

# Каталоги корня. Закрытый список — каждая строка отвечает на вопрос «что это
# делает на первой странице репозитория». Расширять только осознанно.
ALLOWED_DIRS='
.githooks
.github
.sourcecraft
.vscode
bench
compiler-codegen
docker
docs
editors
examples
img
nova-cli
nova-lsp
nova_tests
nova_tests.old
novac
scratch-opencode
scripts
spec
spec_tests
std
THIRD_PARTY
'
# nova_tests.old — отдельная открытая запись №542 (886 файлов, «что его собирает»),
# в списке как факт, не как одобрение. scratch-opencode — штатное рабочее место
# opencode (docs/dev/opencode-runbook.md), в индексе только его .gitignore.

FOUND=$(git -C "$ROOT" ls-files --full-name 2>/dev/null | grep -v '/')
# core.quotepath=off: иначе путь с не-ASCII внутри приходит в кавычках и
# первый сегмент читается как `"spec_tests` — ложный красный на самом себе.
FOUND_DIRS=$(git -C "$ROOT" -c core.quotepath=off ls-files --full-name 2>/dev/null | sed -n 's|^\([^/][^/]*\)/.*|\1|p' | sort -u)
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
BAD_DIRS=""
for d in $FOUND_DIRS; do
    case "$(printf '%s' "$ALLOWED_DIRS" | grep -Fx "$d")" in
        "") BAD_DIRS="$BAD_DIRS $d" ;;
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
if [ -n "$BAD_DIRS" ]; then
    echo "check-repo-root-clean: в корне лежит непредусмотренный КАТАЛОГ:" >&2
    for d in $BAD_DIRS; do echo "    $d/" >&2; done
    echo "" >&2
    echo "    Черновики окна (scratch*/, probes-*/, .pNNN/) в репозиторий не" >&2
    echo "    попадают; улика, на которую ссылается реестр/спека/план, живёт в" >&2
    echo "    docs/plans/repro/ с суффиксом .nv.txt (README там, реестр №695)." >&2
    echo "    Если каталог ДЕЙСТВИТЕЛЬНО должен встречать гостя — впиши его в" >&2
    echo "    ALLOWED_DIRS этого стража и скажи в коммите, почему." >&2
    echo "check-repo-root-clean: FAIL" >&2
    exit 1
fi

echo "check-repo-root-clean ok: в корне только предусмотренное (файлов $(printf '%s\n' $FOUND | wc -l), каталогов $(printf '%s\n' $FOUND_DIRS | wc -l))"
exit 0
