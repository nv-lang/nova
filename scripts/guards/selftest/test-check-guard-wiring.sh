#!/usr/bin/env bash
# test-check-guard-wiring.sh — САМОТЕСТ мета-стража `check-guard-wiring.sh`.
#
# Мета-страж проверяет, что каждый страж документирован/подключён/покрыт. Сам он
# тоже страж, поэтому обязан доказать те же два свойства: ЛОВИТ нарушение и НЕ
# даёт ложного срабатывания. Иначе получилась бы рекурсия доверия на слово.
#
# Дополнительно этот самотест ПРОГОНЯЕТ мета-страж на РЕАЛЬНОЙ репе — так
# правило владельца («нет толку, если не подключён») энфорсится на каждом гейте
# без отдельного шага в gate.sh: цикл `scripts/guards/selftest/test-*.sh`
# подхватывает этот файл автоматически.
#
# Запуск: scripts/guards/selftest/test-check-guard-wiring.sh
# Выход: 0 — мета-страж исправен И реальная репа чиста; 1 — иначе.
#
# План: docs/plans/231-bug-cycle-exit.md §4в.

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]:-$0}")" && pwd)"
# Скрипт живёт в scripts/guards/selftest/ — корень репы на три уровня выше.
REPO_ROOT="$(cd "$SCRIPT_DIR/../../.." && pwd)"
GUARD="$REPO_ROOT/scripts/guards/check-guard-wiring.sh"

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

fails=0
check() { # имя, ожидаемый_код, фактический_код
    if [ "$2" -eq "$3" ]; then
        echo "  ok: $1"
    else
        echo "  ПРОВАЛ: $1 — ожидался код $2, получен $3" >&2
        fails=$((fails + 1))
    fi
}

# Собрать игрушечную репу: scripts/guards/ + scripts/guards/selftest/ + gate.sh
# с циклом самотестов (та же трёхуровневая форма, что настоящая scripts/).
make_repo() { # каталог
    mkdir -p "$1/scripts/guards/selftest"
    cat > "$1/scripts/gate.sh" <<'EOG'
#!/usr/bin/env bash
for st in "$ROOT"/scripts/guards/selftest/test-*.sh; do bash "$st"; done
EOG
}

good_header() { # файл, имя
    cat > "$1" <<EOH
#!/usr/bin/env bash
# $2 — учебный страж для самотеста.
# ПОЧЕМУ: проверяем, что мета-страж принимает корректно оформленного стража.
# ЧТО ПРОВЕРЯЕТ: ничего, это фикстура.
# ИСПОЛЬЗОВАНИЕ: $2
# Коды: 0 — ок.
# Ещё строка шапки, чтобы набрать минимум.
# И ещё одна.
# План: docs/plans/231-bug-cycle-exit.md §4в.
exit 0
EOH
}

echo "самотест check-guard-wiring:"

# (1) ЛОВИТ: страж без самотеста.
r1="$tmp/r1"; make_repo "$r1"
good_header "$r1/scripts/guards/check-foo.sh" "check-foo.sh"
bash "$GUARD" "$r1" >/dev/null 2>&1
check "ловит стража без самотеста" 1 $?

# (2) ЛОВИТ: страж с тонкой шапкой (самотест есть).
r2="$tmp/r2"; make_repo "$r2"
printf '#!/usr/bin/env bash\n# коротко\nexit 0\n' > "$r2/scripts/guards/check-bar.sh"
touch "$r2/scripts/guards/selftest/test-check-bar.sh"
bash "$GUARD" "$r2" >/dev/null 2>&1
check "ловит тонкую шапку" 1 $?

# (3) ЛОВИТ: шапка есть, самотест есть, но нет ссылки на план.
r3="$tmp/r3"; make_repo "$r3"
{ printf '#!/usr/bin/env bash\n'; for i in $(seq 1 10); do printf '# строка шапки %s\n' "$i"; done; printf 'exit 0\n'; } > "$r3/scripts/guards/check-baz.sh"
touch "$r3/scripts/guards/selftest/test-check-baz.sh"
bash "$GUARD" "$r3" >/dev/null 2>&1
check "ловит отсутствие ссылки на план" 1 $?

# (4) НЕ ловит: полностью корректный страж (шапка + план + самотест + цикл в gate).
r4="$tmp/r4"; make_repo "$r4"
good_header "$r4/scripts/guards/check-good.sh" "check-good.sh"
touch "$r4/scripts/guards/selftest/test-check-good.sh"
bash "$GUARD" "$r4" >/dev/null 2>&1
check "НЕ ловит корректно оформленного стража" 0 $?

# (5) НЕ ловит: стражей нет вовсе — нечего проверять.
r5="$tmp/r5"; make_repo "$r5"
bash "$GUARD" "$r5" >/dev/null 2>&1
check "НЕ ловит пустой набор стражей" 0 $?

# (6) РЕАЛЬНАЯ РЕПА: все существующие стражи обязаны быть в порядке.
#     Это и есть подключение правила к автопроверке — падает гейт, а не «когда-нибудь заметим».
bash "$GUARD" >/dev/null 2>&1
check "реальная репа nova: все стражи документированы/подключены/покрыты" 0 $?

if [ "$fails" -ne 0 ]; then
    echo "самотест ПРОВАЛЕН: $fails свойств(а) не выполняются" >&2
    exit 1
fi
echo "самотест ok: мета-страж ловит нарушения, не даёт ложняка, реальная репа чиста"
