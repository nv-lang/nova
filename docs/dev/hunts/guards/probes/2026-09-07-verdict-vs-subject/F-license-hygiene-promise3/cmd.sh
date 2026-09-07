#!/bin/sh
# ПРОБА F — check-license-hygiene.sh
#
# ОБЕЩАНИЕ ШАПКИ (scripts/guards/check-license-hygiene.sh:22-26, дословно):
#   "ЧТО ПРОВЕРЯЕТСЯ:
#      1. У каждого Cargo.toml с секцией `[package]` есть поле `license`.
#      2. У каждого nova.toml с секцией `[package]` есть поле `license`.
#      3. Рядом с каждым таким манифестом лежат ОБА файла лицензии.
#      4. Каждый подмодуль из `.gitmodules` назван в `THIRD_PARTY/README.md`."
#
# ЧТО ПРОВЕРЯЕТ НА ДЕЛЕ. В теле стража три блока: цикл по манифестам
# (пункты 1 и 2) и блок `.gitmodules` (пункт 4). ПУНКТА 3 В КОДЕ НЕТ —
# слово LICENSE в файле не встречается ни разу:
#   grep -n "LICENSE" scripts/guards/check-license-hygiene.sh   -> пусто
#   grep -n "LICENSE" scripts/guards/selftest/test-check-license-hygiene.sh -> пусто
# То есть у обещания нет ни строки кода, ни случая самотеста.
#
# Вердикт при этом сформулирован по факту работы —
#   "check-license-hygiene ok: манифесты объявляют лицензию, подмодули названы"
# — и потому расхождение видно только тому, кто откроет код: читатель шапки
# уверен, что отсутствие LICENSE-MIT/LICENSE-APACHE рядом с пакетом краснит
# гейт. Правило проекта, ради которого пункт 3 и написан, живёт в
# AGENTS.md («License: code is MIT OR Apache-2.0») и в самой шапке стража
# (случай 2026-08: «вендоренный Brotli, MIT, не имел файла уведомлений вовсе»).
#
# ЗАПУСК:  sh cmd.sh [КОРЕНЬ-РЕПОЗИТОРИЯ]
set -u
HERE=$(cd "$(dirname "$0")" && pwd)
REPO="${1:-}"
if [ -z "$REPO" ]; then
    d="$HERE"
    while [ "$d" != "/" ] && [ ! -f "$d/scripts/guards/check-license-hygiene.sh" ]; do
        d=$(dirname "$d")
    done
    REPO="$d"
fi
G="$REPO/scripts/guards/check-license-hygiene.sh"
[ -f "$G" ] || { echo "cmd.sh: не нашёл корень репозитория; передай его аргументом" >&2; exit 2; }

echo "=== ЗАМЕР ПО КОДУ: сколько раз слово LICENSE встречается в страже и его самотесте ==="
printf 'guard    : %s\n' "$(grep -c 'LICENSE' "$G" || true)"
printf 'selftest : %s\n' "$(grep -c 'LICENSE' "$REPO/scripts/guards/selftest/test-check-license-hygiene.sh" || true)"
echo

T="${TMPDIR:-/tmp}/probeF.$$"
rm -rf "$T"; mkdir -p "$T/pkg"

mkmanifest() {
    cat > "$T/pkg/nova.toml" <<EOF
[package]
name = "demo"
version = "0.1.0"
$1
EOF
}

run() {
    printf '%s\n' "--- $1 ---"
    bash "$G" "$T" 2>&1
    printf 'rc=%s\n\n' "$?"
}

mkmanifest 'license = "MIT OR Apache-2.0"'
printf 'MIT text\n'    > "$T/pkg/LICENSE-MIT"
printf 'Apache text\n' > "$T/pkg/LICENSE-APACHE"
run "КОНТРОЛЬ 0: поле license есть, ОБА файла лицензии рядом -> зелёный"

rm -f "$T/pkg/LICENSE-MIT" "$T/pkg/LICENSE-APACHE"
ls "$T/pkg"
run "ДЕФЕКТ: поле license есть, файлов лицензии рядом НЕТ НИ ОДНОГО (пункт 3 шапки)"

mkmanifest '# license field intentionally removed'
run "КОНТРОЛЬ 1: поле license убрано -> КРАСНЫЙ (пункт 2 шапки механизм держит)"

mkmanifest 'license = "MIT OR Apache-2.0"'
printf 'MIT text\n' > "$T/pkg/LICENSE-MIT"
run "КОНТРОЛЬ 2: лежит ТОЛЬКО LICENSE-MIT, второго нет -> зелёный (пункт 3 требует ОБА)"

rm -rf "$T"
