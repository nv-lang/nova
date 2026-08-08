#!/usr/bin/env bash
# Селфтест scripts/guards/check-no-path-deps.sh (D420, реестр 221.1 №444).
#
# Страж без селфтеста не работает — правило владельца 2026-07-27, и мета-страж
# check-guard-wiring его энфорсит (он же поймал этого стража без селфтеста).
# Проверяем ОБА направления: ловит нарушение и не даёт ложных срабатываний.
set -u
export LC_ALL=C

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
G="$ROOT/scripts/guards/check-no-path-deps.sh"
FAILED=0
ok()  { echo "  ok: $1"; }
bad() { echo "  ПРОВАЛ: $1" >&2; FAILED=1; }

echo "== селфтест check-no-path-deps =="

# 1. На реальной репе — зелено (иначе страж непригоден к подключению в гейт).
if bash "$G" "$ROOT" >/dev/null 2>&1; then
    ok "реальная репа nova: нарушений D420 нет"
else
    bad "реальная репа краснит — путь вне [replace] в коммитящемся манифесте/локе"
fi

# 2. Ловит path вне [replace] в манифесте.
T=$(mktemp -d)
git -C "$T" init -q 2>/dev/null
printf '[package]\nname = "x"\n\n[dependencies]\nfoo = { path = "../../foo" }\n' > "$T/nova.toml"
git -C "$T" add nova.toml 2>/dev/null
if bash "$G" "$T" >/dev/null 2>&1; then
    bad "НЕ поймал path в [dependencies]"
else
    ok "ловит path вне [replace] (код 1)"
fi

# 3. НЕ ругается на path ВНУТРИ [replace] — там он законен по D420.
printf '[package]\nname = "x"\n\n[dependencies]\nfoo = { git = "https://e/f", version = "0.1" }\n\n[replace]\nfoo = { path = "../../foo" }\n' > "$T/nova.toml"
git -C "$T" add nova.toml 2>/dev/null
if bash "$G" "$T" >/dev/null 2>&1; then
    ok "не ругается на path внутри [replace]"
else
    bad "ложное срабатывание на законном [replace]"
fi

# 4. Ловит путевой источник в КОММИТЯЩЕМСЯ лок-файле.
printf '[package]\nname = "x"\n\n[dependencies]\nfoo = { git = "https://e/f", version = "0.1" }\n' > "$T/nova.toml"
printf 'version = 1\n\n[[package]]\nname = "foo"\nsource = "path"\npath = "../../foo"\n' > "$T/nova.lock.toml"
git -C "$T" add nova.toml nova.lock.toml 2>/dev/null
if bash "$G" "$T" >/dev/null 2>&1; then
    bad "НЕ поймал путевой источник в локе"
else
    ok "ловит путевой источник в локе (код 1)"
fi

rm -rf "$T"

if [ "$FAILED" -eq 0 ]; then
    echo "селфтест check-no-path-deps: 4/4 ok"
    exit 0
fi
echo "селфтест check-no-path-deps: ЕСТЬ ПРОВАЛЫ" >&2
exit 1
