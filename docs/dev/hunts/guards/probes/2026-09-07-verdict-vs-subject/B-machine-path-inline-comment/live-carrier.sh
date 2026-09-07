#!/bin/sh
# ПРОБА B, часть 2 — ЖИВОЙ НОСИТЕЛЬ в дереве на 2026-09-07.
#
# scripts/claude-hooks/selftest/test-guard-git-commit-scope.py:17
#     R = "/d/Sources/nv-lang/nova"  # дерево без слияния в процессе
#
# Строка исполняемая (значение используется во всех 13 случаях самотеста),
# путь к машине литеральный, ИМЕНОВАННОЙ отметки `machine-path-fixture`
# на ней НЕТ — в отличие от трёх соседних файлов, где такая отметка стоит:
#     test-guard-git-powershell.py:41,43  ...  # machine-path-fixture
#     test-guard-shell-nonascii.py:48     ...  # machine-path-fixture
# Страж зелен только из-за хвостового комментария (фильтр 'решётка+пробел').
#
# Шапка САМОГО файла-носителя, строки 9-10, дословно:
#   "# Путь к хуку выводится от расположения теста — литеральный путь к машине
#    # автора в отслеживаемом скрипте это класс №698 (см. check-no-machine-paths)."
# То есть файл объявляет соблюдение №698 и нарушает его следующей же строкой.
#
# ЭТА ПРОБА НИЧЕГО НЕ ПРАВИТ В ДЕРЕВЕ: файл КОПИРУЕТСЯ во временный репозиторий.
#
# ЗАПУСК:  sh live-carrier.sh [КОРЕНЬ-РЕПОЗИТОРИЯ]
set -u
PYTHONIOENCODING=utf-8; export PYTHONIOENCODING
HERE=$(cd "$(dirname "$0")" && pwd)
REPO="${1:-}"
if [ -z "$REPO" ]; then
    d="$HERE"
    while [ "$d" != "/" ] && [ ! -f "$d/scripts/guards/check-no-machine-paths.sh" ]; do
        d=$(dirname "$d")
    done
    REPO="$d"
fi
G="$REPO/scripts/guards/check-no-machine-paths.sh"
SRC="$REPO/scripts/claude-hooks/selftest/test-guard-git-commit-scope.py"
[ -f "$G" ] && [ -f "$SRC" ] || { echo "cmd.sh: не нашёл корень репозитория; передай его аргументом" >&2; exit 2; }

echo "=== строка-носитель в ЖИВОМ дереве (только чтение) ==="
sed -n '9,17p' "$SRC"
echo
echo "=== как отвечает страж на ЖИВОМ дереве (чистый замер, без правок) ==="
bash "$G" "$REPO" 2>&1 | tail -2
echo "rc=$?"
echo

T="${TMPDIR:-/tmp}/probeB2.$$"
rm -rf "$T"; mkdir -p "$T/scripts/claude-hooks/selftest"
git -C "$T" init -q
git -C "$T" config user.email t@t
git -C "$T" config user.name t
cp "$SRC" "$T/scripts/claude-hooks/selftest/test-guard-git-commit-scope.py"
git -C "$T" add -A >/dev/null 2>&1

echo "--- КОПИЯ носителя как есть -> ожидается зелёный (это и есть дефект) ---"
bash "$G" "$T" 2>&1; echo "rc=$?"
echo

python - "$T/scripts/claude-hooks/selftest/test-guard-git-commit-scope.py" <<'PYEOF'
import io, sys
p = sys.argv[1]
s = io.open(p, encoding="utf-8").read()
old = u'R = "/d/Sources/nv-lang/nova"  # '
i = s.find(old)
assert i >= 0, "якорь не найден: строка-носитель изменилась"
j = s.index(u"\n", i)
s = s[:i] + u'R = "/d/Sources/nv-lang/nova"' + s[j:]
io.open(p, "w", encoding="utf-8", newline="\n").write(s)
print("в КОПИИ снят ТОЛЬКО хвостовой комментарий, путь не тронут")
PYEOF
git -C "$T" add -A >/dev/null 2>&1

echo "--- КОНТРОЛЬ: у копии снят ТОЛЬКО хвостовой комментарий -> ожидается КРАСНЫЙ ---"
bash "$G" "$T" 2>&1; echo "rc=$?"

rm -rf "$T"
