#!/bin/sh
# ПРОБА B — check-no-machine-paths.sh
#
# ОБЕЩАНИЕ ШАПКИ (scripts/guards/check-no-machine-paths.sh, дословно):
#   "ЧТО СЧИТАЕТСЯ ПУТЁМ К МАШИНЕ: `d:/Sources`, `D:\Sources`, `/d/Sources`,
#    `/mnt/d/Sources`, `C:\Users\<имя>` — в строке, которая НЕ комментарий."
# СТРОКА ВЕРДИКТА:
#   "$NAME ok: скриптов проверено $N, путей к машине вне комментариев 0 (№698)"
#
# ЧТО ФИЛЬТРУЕТ НА ДЕЛЕ (тот же файл, конвейер RAW=...):
#   | grep -vE '^[^:]*:[0-9]+:[[:space:]]*#'            <- настоящие комментарии
#   | grep -vE 'Проверялся|проверялся|# |Prichina:|machine-path-fixture|^[^:]*:[0-9]+:step '
#                                        ^^^
# Второй фильтр выбрасывает ЛЮБУЮ строку, где ГДЕ УГОДНО встречается решётка
# с пробелом. Исполняемая строка с ХВОСТОВЫМ комментарием — не комментарий,
# но выброшена. Тот же фильтр выбрасывает строку со словом «проверялся»
# в любом месте.
#
# СВОЙСТВО (не конструкция): «строка исполняемого скрипта несёт абсолютный путь
# к машине». Синтаксисов у него минимум четыре — ловится один.
#
# ЗАПУСК:  sh cmd.sh [КОРЕНЬ-РЕПОЗИТОРИЯ]
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
[ -f "$G" ] || { echo "cmd.sh: не нашёл корень репозитория; передай его аргументом" >&2; exit 2; }

T="${TMPDIR:-/tmp}/probeB.$$"
rm -rf "$T"; mkdir -p "$T/scripts/tools" "$T/scripts/githooks"
git -C "$T" init -q
git -C "$T" config user.email t@t
git -C "$T" config user.name t

run() {
    printf '%s\n' "--- $1 ---"
    git -C "$T" add -A >/dev/null 2>&1
    sh "$G" "$T" 2>&1
    printf 'rc=%s\n\n' "$?"
}

# ------------------------------------------------------------------ КОНТРОЛЬ 0
cat > "$T/scripts/tools/x.sh" <<'EOF'
#!/bin/sh
ROOT=$(cd "$(dirname "$0")/../.." && pwd)
echo "$ROOT"
EOF
run "КОНТРОЛЬ 0: чистый скрипт, путей нет -> ожидается зелёный"

# ------------------------------------------------------------------ КОНТРОЛЬ 1
cat > "$T/scripts/tools/x.sh" <<'EOF'
#!/bin/sh
NOVA_GC_LIB_DIR="D:/Sources/nv-lang/nova/compiler-codegen/nova_rt"
echo "$NOVA_GC_LIB_DIR"
EOF
run "КОНТРОЛЬ 1: НОСИТЕЛЬ №698 голой строкой -> ожидается КРАСНЫЙ"

# ------------------------------------------------------------------ ДЕФЕКТ 1
cat > "$T/scripts/tools/x.sh" <<'EOF'
#!/bin/sh
NOVA_GC_LIB_DIR="D:/Sources/nv-lang/nova/compiler-codegen/nova_rt" # GC lives here
echo "$NOVA_GC_LIB_DIR"
EOF
run "ДЕФЕКТ 1: ТА ЖЕ строка + хвостовой комментарий '# GC lives here'"

# ------------------------------------------------------------------ КОНТРОЛЬ 2
cat > "$T/scripts/tools/x.sh" <<'EOF'
#!/bin/sh
NOVA_GC_LIB_DIR="D:/Sources/nv-lang/nova/compiler-codegen/nova_rt" #GCliveshere
echo "$NOVA_GC_LIB_DIR"
EOF
run "КОНТРОЛЬ 2: тот же хвост БЕЗ пробела после решётки -> КРАСНЫЙ (значит спусковой крючок — ровно 'решётка+пробел')"

# ------------------------------------------------------------------ ДЕФЕКТ 2
cp "$REPO/scripts/guards/check-no-machine-paths.sh" /dev/null 2>/dev/null
python - "$T/scripts/tools/x.sh" <<'PYEOF'
import io, sys
io.open(sys.argv[1], "w", encoding="utf-8", newline="\n").write(
    u'#!/bin/sh\n'
    u'NOVA_GC_LIB_DIR="D:/Sources/nv-lang/nova/compiler-codegen/nova_rt"  '
    u'&& echo "\u043f\u0440\u043e\u0432\u0435\u0440\u044f\u043b\u0441\u044f"\n')
PYEOF
run "ДЕФЕКТ 2: та же строка + слово 'проверялся' где угодно в ней"

# ------------------------------------------------------------------ ДЕФЕКТ 3
cat > "$T/scripts/tools/x.sh" <<'EOF'
#!/bin/sh
echo clean
EOF
cat > "$T/scripts/githooks/pre-commit" <<'EOF'
#!/bin/sh
NOVA_GC_LIB_DIR="D:/Sources/nv-lang/nova/compiler-codegen/nova_rt"
exec "$NOVA_GC_LIB_DIR/hook"
EOF
chmod +x "$T/scripts/githooks/pre-commit"
run "ДЕФЕКТ 3: та же строка в scripts/githooks/pre-commit (исполняемый скрипт БЕЗ расширения)"

# ------------------------------------------------------------------ КОНТРОЛЬ 3
mv "$T/scripts/githooks/pre-commit" "$T/scripts/githooks/pre-commit.sh"
git -C "$T" rm -q --cached scripts/githooks/pre-commit >/dev/null 2>&1
run "КОНТРОЛЬ 3: ТОТ ЖЕ файл под именем pre-commit.sh -> КРАСНЫЙ"

rm -rf "$T"
