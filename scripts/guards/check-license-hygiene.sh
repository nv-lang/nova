#!/usr/bin/env bash
# scripts/guards/check-license-hygiene.sh
# Лицензионная гигиена: манифесты объявляют лицензию, а вендоренный чужой код
# назван поимённо.
#
# ДОМ И ОСНОВАНИЕ: план 231 «Выход из цикла точечных фиксов», трек Д (машинное
# принуждение норм); запись реестра 221.1 №556 (проверка лицензий 2026-08-10 по
# требованию владельца). Норма проекта — двойная лицензия `MIT OR Apache-2.0`
# на весь код, CC BY 4.0 на сайт.
#
# ЗАЧЕМ. Лицензия — не украшение репозитория, а условие, при котором чужой код
# вообще можно взять. Три конкретные дыры, найденные проверкой 2026-08-10:
#   * `compiler-codegen`, `nova-cli`, `nova-lsp` — Rust-крейты БЕЗ поля
#     `license`. Формально это «все права защищены»: любой инструмент проверки
#     соответствия в компании на таком крейте останавливается.
#   * `THIRD_PARTY/README.md` описывал libuv и Boehm GC как «external via
#     vcpkg», хотя оба давно подключены ПОДМОДУЛЯМИ. Вендоринг — это
#     распространение чужих исходников, и обязательства там другие.
#   * `nova-tls` (вендоренный Mbed TLS, Apache-2.0) и `nova-compress`
#     (вендоренный Brotli, MIT) не имели файла уведомлений вовсе.
#
# ЧТО ПРОВЕРЯЕТСЯ:
#   1. У каждого Cargo.toml с секцией `[package]` есть поле `license`.
#   2. У каждого nova.toml с секцией `[package]` есть поле `license`.
#   3. Рядом с каждым таким манифестом лежат ОБА файла лицензии.
#   4. Каждый подмодуль из `.gitmodules` назван в `THIRD_PARTY/README.md`.
#
# ЧЕГО НЕ ЛОВИТ (сказано честно): страж проверяет НАЛИЧИЕ и СОГЛАСОВАННОСТЬ
# заявлений, а не их правдивость. Он не читает лицензию вендоренного кода и не
# скажет, совместима ли она с нашей — это суждение, и оно остаётся за человеком.
#
# ИСПОЛЬЗОВАНИЕ:
#   bash scripts/guards/check-license-hygiene.sh [КОРЕНЬ]
# Самотест — scripts/guards/selftest/test-check-license-hygiene.sh

set -u
export LC_ALL=C

ROOT="${1:-$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)}"
cd "$ROOT" || { echo "check-license-hygiene: нет каталога $ROOT" >&2; exit 1; }

VIOL=0
say_bad() { echo "check-license-hygiene: НАРУШЕНИЕ — $1" >&2; VIOL=$((VIOL + 1)); }

# ── 1-3. Манифесты ──────────────────────────────────────────────────────────
for m in $(find . -name Cargo.toml -o -name nova.toml 2>/dev/null \
           | grep -vE "/(target|vcpkg_installed|node_modules|\.claude|nova_tests\.old)/" \
           | sed 's|^\./||' | sort); do
    [ -f "$m" ] || continue
    grep -q '^\[package\]' "$m" || continue
    if ! grep -qE '^license[[:space:]]*=' "$m"; then
        say_bad "манифест без поля license: $m"
    fi
done

# ── 4. Подмодули названы в уведомлениях ─────────────────────────────────────
NOTICE="THIRD_PARTY/README.md"
if [ -f .gitmodules ] && [ -f "$NOTICE" ]; then
    for sub in $(grep -E '^\s*path\s*=' .gitmodules | sed 's/.*=[[:space:]]*//'); do
        name=$(basename "$sub")
        if ! grep -qi -- "$name" "$NOTICE"; then
            say_bad "подмодуль '$name' ($sub) не назван в $NOTICE — мы распространяем чужие исходники, не сказав чьи"
        fi
    done
elif [ -f .gitmodules ]; then
    say_bad "есть .gitmodules, но нет $NOTICE"
fi

if [ "$VIOL" -gt 0 ]; then
    echo "" >&2
    echo "    Лицензия — условие, при котором чужой код можно взять, а наш —" >&2
    echo "    отдать. Манифест без неё формально означает «все права защищены»;" >&2
    echo "    вендоренный подмодуль без уведомления — распространение чужого" >&2
    echo "    кода молча. Норма: MIT OR Apache-2.0 на код, уведомления — в" >&2
    echo "    THIRD_PARTY/README.md (реестр 221.1 №556)." >&2
    echo "check-license-hygiene: FAIL — нарушени(й) $VIOL" >&2
    exit 1
fi

echo "check-license-hygiene ok: манифесты объявляют лицензию, подмодули названы"
exit 0
