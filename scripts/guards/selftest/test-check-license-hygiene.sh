#!/usr/bin/env bash
# Селфтест scripts/guards/check-license-hygiene.sh.
#
# Обе стороны обязательны. Страж, краснеющий на правильном дереве, будет
# отключён в первый же день — и норма умрёт вместе с ним.
set -u
export LC_ALL=C

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
G="$ROOT/scripts/guards/check-license-hygiene.sh"
FAILED=0
ok()  { echo "  ok: $1"; }
bad() { echo "  ПРОВАЛ: $1" >&2; FAILED=1; }

TMP=$(mktemp -d); trap 'rm -rf "$TMP"' EXIT
mkdir -p "$TMP/crate" "$TMP/THIRD_PARTY"

# 1. Манифест с лицензией — зелено.
printf '[package]\nname = "x"\nlicense = "MIT OR Apache-2.0"\n' > "$TMP/crate/Cargo.toml"
out=$(bash "$G" "$TMP" 2>&1); rc=$?
if [ "$rc" -eq 0 ]; then ok "манифест с лицензией проходит"; else bad "ложный отказ: $out"; fi

# 2. Манифест БЕЗ лицензии — красно, с именем файла.
printf '[package]\nname = "x"\n' > "$TMP/crate/Cargo.toml"
out=$(bash "$G" "$TMP" 2>&1); rc=$?
if [ "$rc" -eq 1 ] && echo "$out" | grep -q "crate/Cargo.toml"; then ok "ловит манифест без лицензии"; else bad "не поймал отсутствие лицензии (код $rc): $out"; fi

# 3. То же для nova.toml — правило одно на оба вида манифестов.
rm -f "$TMP/crate/Cargo.toml"
printf '[package]\nname = "p"\n' > "$TMP/crate/nova.toml"
out=$(bash "$G" "$TMP" 2>&1); rc=$?
if [ "$rc" -eq 1 ] && echo "$out" | grep -q "crate/nova.toml"; then ok "ловит nova.toml без лицензии"; else bad "не поймал nova.toml (код $rc): $out"; fi

# 4. Подмодуль, не названный в уведомлениях, — красно.
printf '[package]\nname = "p"\nlicense = "MIT OR Apache-2.0"\n' > "$TMP/crate/nova.toml"
printf '[submodule "vendor/somelib"]\n\tpath = vendor/somelib\n\turl = https://example.invalid/somelib.git\n' > "$TMP/.gitmodules"
printf '# Third-Party Licenses\n\nnothing here\n' > "$TMP/THIRD_PARTY/README.md"
out=$(bash "$G" "$TMP" 2>&1); rc=$?
if [ "$rc" -eq 1 ] && echo "$out" | grep -q "somelib"; then ok "ловит неназванный подмодуль"; else bad "не поймал подмодуль (код $rc): $out"; fi

# 5. Названный подмодуль — зелено (не ложняк).
printf '# Third-Party Licenses\n\n### somelib — vendored as a submodule, MIT\n' > "$TMP/THIRD_PARTY/README.md"
out=$(bash "$G" "$TMP" 2>&1); rc=$?
if [ "$rc" -eq 0 ]; then ok "названный подмодуль не краснит"; else bad "ложный отказ на названном подмодуле (код $rc): $out"; fi

# 6. Есть подмодули, но нет файла уведомлений вовсе — красно.
rm -f "$TMP/THIRD_PARTY/README.md"
out=$(bash "$G" "$TMP" 2>&1); rc=$?
if [ "$rc" -eq 1 ] && echo "$out" | grep -q "THIRD_PARTY"; then ok "ловит отсутствие файла уведомлений"; else bad "не поймал отсутствие уведомлений (код $rc): $out"; fi

if [ "$FAILED" -eq 0 ]; then echo "селфтест check-license-hygiene: 6/6 ok"; exit 0; fi
echo "селфтест check-license-hygiene: ЕСТЬ ПРОВАЛЫ" >&2
exit 1
