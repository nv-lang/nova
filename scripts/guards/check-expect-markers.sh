#!/usr/bin/env bash
# scripts/guards/check-expect-markers.sh — неизвестный `EXPECT_*` в тесте.
#
# №453 (часть 3): `parse_expect` (`test_runner.rs:306-332`) знает РОВНО семь имён,
# всё прочее молча падает в `else { None }`. Следствие: файл с опечаткой в
# маркере ЗЕЛЁН и не проверяет НИЧЕГО. Найдено аудитом 2026-08-08: в корпусе жили
# `EXPECT_OUTPUT` (5) и `EXPECT_RUN_OK` (3) — восемь фикстур-пустышек.
#
# Раннер молчит по построению, поэтому проверяем снаружи: любой `// EXPECT_*`,
# которого нет в белом списке, — красный гейт.
set -u
export LC_ALL=C
ROOT="${1:-$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)}"
cd "$ROOT" || exit 1

KNOWN='EXPECT_COMPILE_ERROR|EXPECT_CC_ERROR|EXPECT_RUNTIME_PANIC|EXPECT_EXIT_CODE|EXPECT_STDOUT|EXPECT_STDERR|EXPECT_COMPILE_WARNING|EXPECT_TIMEOUT_MS|EXPECT_TIMEOUT|EXPECT_EXIT'

BAD=$(grep -rhoE '^[[:space:]]*//[[:space:]]*EXPECT_[A-Z_]+' \
        spec_tests std examples 2>/dev/null \
      | sed 's|.*//[[:space:]]*||' | sort -u | grep -vE "^($KNOWN)$")

if [ -n "$BAD" ]; then
    echo "check-expect-markers: НЕИЗВЕСТНЫЕ маркеры (раннер их МОЛЧА игнорирует):" >&2
    echo "$BAD" | sed 's/^/    /' >&2
    echo "    известные: $(echo "$KNOWN" | tr '|' ' ')" >&2
    echo "check-expect-markers: FAIL" >&2
    exit 1
fi
echo "check-expect-markers ok: неизвестных EXPECT_* нет"
exit 0
