#!/usr/bin/env bash
# scripts/guards/check-manifest-language.sh
# Манифесты пакетов — по-английски (решение владельца 2026-08-10).
#
# ЗАЧЕМ. `nova.toml` и `nova.lock.toml` — самые публичные файлы, какие у нас
# есть: они уезжают в пакетные репозитории, их открывает первым делом всякий,
# кто смотрит на пакет, и они цитируются в сообщениях об ошибках резолвера.
# Комментарий на русском в таком файле сужает круг читателей до одного
# человека. Та же норма, что для сообщений коммитов (2026-08-09,
# check-commit-language.sh) — репозиторий публичный, историю и манифесты
# читают снаружи.
#
# ЧТО ПРОВЕРЯЕТСЯ: ни одной кириллической буквы в отслеживаемых `nova.toml` и
# `nova.lock.toml`.
#
# ЧТО НЕ ПРОВЕРЯЕТСЯ И ПОЧЕМУ: `nova_tests.old/**` исключён. Это мёртвое
# дерево, которое не собирает ни гейт, ни CI (запись реестра 221.1 №542 —
# «код, который ничто не собирает, гниёт»); переводить его комментарии значит
# полировать то, что подлежит удалению. Когда №542 закроется решением по
# судьбе этих каталогов — исключение снимается вместе с ними.
#
# ИСПОЛЬЗОВАНИЕ:
#   bash scripts/guards/check-manifest-language.sh [КОРЕНЬ]
# Самотест — scripts/guards/selftest/test-check-manifest-language.sh

set -u
export LC_ALL=C

ROOT="${1:-$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)}"
cd "$ROOT" || { echo "check-manifest-language: нет каталога $ROOT" >&2; exit 1; }

# Список файлов: отслеживаемые git'ом манифесты. Если git недоступен (случай
# самотеста на голом каталоге) — обходим файловую систему.
if git -C "$ROOT" rev-parse --git-dir >/dev/null 2>&1; then
    FILES=$(git -C "$ROOT" ls-files "*nova.toml" "*nova.lock.toml")
else
    FILES=$(find "$ROOT" -name "nova.toml" -o -name "nova.lock.toml" | sed "s|^$ROOT/||")
fi

BAD=""
N=0
for f in $FILES; do
    case "$f" in
        nova_tests.old/*) continue ;;
    esac
    [ -f "$ROOT/$f" ] || continue
    N=$((N + 1))
    if grep -qP '(*UTF8)\p{Cyrillic}' "$ROOT/$f" 2>/dev/null; then
        BAD="$BAD $f"
    fi
done

echo "check-manifest-language: проверено манифестов $N"

if [ -n "$BAD" ]; then
    echo "check-manifest-language: FAIL — кириллица в манифестах:" >&2
    for f in $BAD; do
        echo "  $f" >&2
        grep -nP '(*UTF8)\p{Cyrillic}' "$ROOT/$f" | head -3 | sed 's/^/      /' >&2
    done
    echo "" >&2
    echo "    Манифест — самый публичный файл пакета: он уезжает в пакетную" >&2
    echo "    репозиторию и его читают снаружи. Норма владельца 2026-08-10:" >&2
    echo "    комментарии в nova.toml/nova.lock.toml — по-английски." >&2
    exit 1
fi

echo "check-manifest-language ok: манифесты по-английски"
exit 0
