#!/usr/bin/env bash
# Селфтест scripts/guards/check-manifest-language.sh.
#
# Проверяем ОБА направления. Второе не менее важно: страж, краснеющий на
# правильном манифесте, будет отключён в первый же день, и норма умрёт вместе
# с ним.
set -u
export LC_ALL=C

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
G="$ROOT/scripts/guards/check-manifest-language.sh"
FAILED=0
ok()  { echo "  ok: $1"; }
bad() { echo "  ПРОВАЛ: $1" >&2; FAILED=1; }

TMP=$(mktemp -d); trap 'rm -rf "$TMP"' EXIT

# 1. Английский манифест — зелено.
mkdir -p "$TMP/pkg"
printf '# a package manifest, all English\n[package]\nname = "pkg"\n' > "$TMP/pkg/nova.toml"
out=$(bash "$G" "$TMP" 2>&1); rc=$?
if [ "$rc" -eq 0 ]; then ok "английский манифест проходит"; else bad "ложный отказ на английском манифесте: $out"; fi

# 2. Кириллица в nova.toml — красно, с именем файла.
printf '# \xd0\xba\xd0\xbe\xd0\xbc\xd0\xbc\xd0\xb5\xd0\xbd\xd1\x82\xd0\xb0\xd1\x80\xd0\xb8\xd0\xb9\n[package]\nname = "pkg"\n' > "$TMP/pkg/nova.toml"
out=$(bash "$G" "$TMP" 2>&1); rc=$?
if [ "$rc" -eq 1 ] && echo "$out" | grep -q "pkg/nova.toml"; then ok "ловит кириллицу в nova.toml"; else bad "не поймал кириллицу (код $rc): $out"; fi

# 3. Кириллица в nova.lock.toml — тоже красно (lock проверяется наравне).
printf '# an English manifest\n' > "$TMP/pkg/nova.toml"
printf '# \xd0\xb7\xd0\xb0\xd0\xbc\xd0\xbe\xd0\xba\n' > "$TMP/pkg/nova.lock.toml"
out=$(bash "$G" "$TMP" 2>&1); rc=$?
if [ "$rc" -eq 1 ] && echo "$out" | grep -q "nova.lock.toml"; then ok "ловит кириллицу в nova.lock.toml"; else bad "не поймал кириллицу в lock (код $rc): $out"; fi

# 4. nova_tests.old/** исключён намеренно — мёртвое дерево (реестр №542).
rm -f "$TMP/pkg/nova.lock.toml"
mkdir -p "$TMP/nova_tests.old/x"
printf '# \xd1\x81\xd1\x82\xd0\xb0\xd1\x80\xd1\x8b\xd0\xb9 \xd1\x82\xd0\xb5\xd1\x81\xd1\x82\n' > "$TMP/nova_tests.old/x/nova.toml"
out=$(bash "$G" "$TMP" 2>&1); rc=$?
if [ "$rc" -eq 0 ]; then ok "мёртвое дерево nova_tests.old не краснит"; else bad "исключение не работает (код $rc): $out"; fi

# 5. Счётчик проверенных файлов не врёт: два живых манифеста → «проверено 2».
printf '# second package\n' > "$TMP/pkg2_nova.toml"
mkdir -p "$TMP/pkg2"; printf '# second package\n' > "$TMP/pkg2/nova.toml"
out=$(bash "$G" "$TMP" 2>&1); rc=$?
if [ "$rc" -eq 0 ] && echo "$out" | grep -q "проверено манифестов 2"; then ok "счётчик считает живые манифесты"; else bad "счётчик врёт (код $rc): $out"; fi

if [ "$FAILED" -eq 0 ]; then echo "селфтест check-manifest-language: 5/5 ok"; exit 0; fi
echo "селфтест check-manifest-language: ЕСТЬ ПРОВАЛЫ" >&2
exit 1
