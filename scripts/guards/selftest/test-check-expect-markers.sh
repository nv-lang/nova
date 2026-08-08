#!/usr/bin/env bash
# Селфтест scripts/guards/check-expect-markers.sh (№453 часть 3).
set -u
export LC_ALL=C

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
G="$ROOT/scripts/guards/check-expect-markers.sh"
FAILED=0
ok()  { echo "  ok: $1"; }
bad() { echo "  ПРОВАЛ: $1" >&2; FAILED=1; }

echo "== селфтест check-expect-markers =="

# 1. Реальная репа — зелено (иначе страж непригоден к подключению).
if bash "$G" "$ROOT" >/dev/null 2>&1; then
    ok "реальная репа: неизвестных маркеров нет"
else
    bad "реальная репа краснит"
fi

T=$(mktemp -d); mkdir -p "$T/spec_tests"

# 2. Ловит неизвестный маркер.
printf '// EXPECT_VYDUMANNYJ: ok\nmodule x\n' > "$T/spec_tests/a.nv"
if bash "$G" "$T" >/dev/null 2>&1; then
    bad "НЕ поймал неизвестный EXPECT_*"
else
    ok "ловит неизвестный EXPECT_* (код 1)"
fi

# 3. Известные маркеры принимаются — включая EXPECT_TIMEOUT_MS (бюджет, не лейн).
printf '// EXPECT_STDOUT: ok\n// EXPECT_TIMEOUT_MS 30000\nmodule x\n' > "$T/spec_tests/a.nv"
if bash "$G" "$T" >/dev/null 2>&1; then
    ok "принимает известные, включая EXPECT_TIMEOUT_MS"
else
    bad "ложно краснит на известных маркерах"
fi

# 4. Упоминание в прозе НЕ в начале строки — не маркер, не ловится.
printf 'module x\n// см. описание EXPECT_STDOUT в конвенции\n' > "$T/spec_tests/a.nv"
if bash "$G" "$T" >/dev/null 2>&1; then
    ok "не ловит упоминание внутри прозы"
else
    bad "ложно краснит на упоминании в прозе"
fi

rm -rf "$T"

if [ "$FAILED" -eq 0 ]; then
    echo "селфтест check-expect-markers: 4/4 ok"
    exit 0
fi
echo "селфтест check-expect-markers: ЕСТЬ ПРОВАЛЫ" >&2
exit 1
