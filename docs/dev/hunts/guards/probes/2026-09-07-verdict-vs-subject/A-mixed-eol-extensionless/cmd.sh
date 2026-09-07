#!/bin/sh
# ПРОБА A — check-mixed-eol.sh
#
# ОБЕЩАНИЕ ШАПКИ (scripts/guards/check-mixed-eol.sh:2, дословно):
#   "Страж: в рабочем дереве нет файлов со СМЕШАННЫМИ окончаниями строк."
# СТРОКА ВЕРДИКТА (там же, последняя):
#   "$NAME ok: смешанных окончаний строк в рабочем дереве нет"
#
# ЧТО ПРОВЕРЯЕТ НА ДЕЛЕ (scripts/guards/mixed-eol-scan.py:22 и :26):
#   EXTS = (".nv",".rs",".md",".sh",".py",".toml",".c",".h",".txt",
#           ".json",".yml",".yaml",".baseline",".list")
#   SKIP_DIRS = ("target",".git","node_modules","vcpkg_installed",".claude",
#                "nova_tests.old","out","dist")
#   ... if not fn.endswith(EXTS): continue
# То есть файл БЕЗ РАСШИРЕНИЯ не судится вовсе, и весь .claude/ тоже.
# scripts/githooks/{pre-commit,commit-msg,pre-push,post-merge,pre-merge-commit}
# — исполняемые shell-скрипты без расширения, которые git запускает на каждом
# коммите. Их же шапка соседа check-script-eol-pinned.py называет опаснейшим
# классом: "шелл-скрипты БЕЗ РАСШИРЕНИЯ ... Их поломка выглядела бы как
# «git сломался»".
#
# ЗАПУСК:  sh cmd.sh [КОРЕНЬ-РЕПОЗИТОРИЯ]
# Без аргумента корень ищется вверх по дереву от каталога пробы.
set -u
HERE=$(cd "$(dirname "$0")" && pwd)
REPO="${1:-}"
if [ -z "$REPO" ]; then
    d="$HERE"
    while [ "$d" != "/" ] && [ ! -f "$d/scripts/guards/check-mixed-eol.sh" ]; do
        d=$(dirname "$d")
    done
    REPO="$d"
fi
[ -f "$REPO/scripts/guards/check-mixed-eol.sh" ] || { echo "cmd.sh: не нашёл корень репозитория; передай его аргументом" >&2; exit 2; }

T="${TMPDIR:-/tmp}/probeA.$$"
rm -rf "$T"; mkdir -p "$T/scripts/guards" "$T/scripts/githooks"
# ядро копируется, а не правится: страж читает его по пути $ROOT/scripts/guards/
cp "$REPO/scripts/guards/mixed-eol-scan.py" "$T/scripts/guards/mixed-eol-scan.py"

# один и тот же байтовый мусор в двух видах: без расширения и с ".sh"
mkbad() { printf '#!/bin/sh\r\necho one\r\necho TWO-with-bare-LF\necho three\r\n' > "$1"; }

echo "=== ДЕФЕКТ: смешанные окончания в scripts/githooks/pre-commit (без расширения) ==="
mkbad "$T/scripts/githooks/pre-commit"
od -c "$T/scripts/githooks/pre-commit" | sed -n '1,4p'
bash "$REPO/scripts/guards/check-mixed-eol.sh" "$T"; echo "rc=$?"

echo
echo "=== КОНТРОЛЬ 1: ТОТ ЖЕ файл, переименован в pre-commit.sh ==="
rm -f "$T/scripts/githooks/pre-commit"
mkbad "$T/scripts/githooks/pre-commit.sh"
bash "$REPO/scripts/guards/check-mixed-eol.sh" "$T"; echo "rc=$?"

echo
echo "=== КОНТРОЛЬ 2: pre-commit.sh с ОДНОРОДНЫМИ CRLF (исправный) ==="
printf '#!/bin/sh\r\necho one\r\necho two\r\n' > "$T/scripts/githooks/pre-commit.sh"
bash "$REPO/scripts/guards/check-mixed-eol.sh" "$T"; echo "rc=$?"

echo
echo "=== КОНТРОЛЬ 3: тот же дефект в .claude/commands/x.md (SKIP_DIRS) ==="
rm -f "$T/scripts/githooks/pre-commit.sh"
mkdir -p "$T/.claude/commands" "$T/docs"
mkbad "$T/.claude/commands/x.md"
bash "$REPO/scripts/guards/check-mixed-eol.sh" "$T"; echo "rc=$?"

echo
echo "=== КОНТРОЛЬ 4: тот же дефект в docs/x.md (судимый каталог) ==="
mkbad "$T/docs/x.md"
bash "$REPO/scripts/guards/check-mixed-eol.sh" "$T"; echo "rc=$?"

rm -rf "$T"
