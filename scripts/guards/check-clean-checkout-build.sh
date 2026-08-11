#!/usr/bin/env bash
# scripts/guards/check-clean-checkout-build.sh
# Флагман собирается на ЧИСТОМ дереве — том, что видит любой посторонний.
#
# ДОМ И ОСНОВАНИЕ: план 231, трек Д (машинное принуждение норм); записи реестра
# 221.1 №283 (dev-override маскировал несобираемость) и №565 (локальный гейт
# оказался слабее внешнего ровно в этом месте).
#
# ЗАЧЕМ — случай 2026-08-10, дословно. Локальный гейт был ЗЕЛЁНЫМ, а CI на том
# же коммите КРАСНЫМ: пять ошибок импорта из `tls`. Причина: у владельца активен
# `nova.override.toml` (законный рабочий инструмент — путь-оверрайд поверх
# запиненной git-зависимости), и `tls` резолвился в соседнюю рабочую копию с
# уже мигрированным кодом. CI же резолвит из `nova.lock.toml`, где висел тег
# `v0.1.6` — выпущенный ДО миграции D452. Тег пакета не двигает потребителя:
# перерезолвить может только `nova update`, и этого шага никто не сделал.
#
# Гейт про override ПРЕДУПРЕЖДАЛ, но предупреждение не отказ: прогон
# засчитывался зелёным. Предупреждение, которое можно не заметить, — это не
# механизм.
#
# ЧТО ДЕЛАЕТ. Заводит ВРЕМЕННОЕ рабочее дерево из текущего HEAD (там нет
# `nova.override.toml`: он в `.gitignore`, значит в дерево не попадает) и
# собирает в нём ОДНУ флагман-цель. Это ровно то, что делает любой чистый
# checkout — CI, клон, worktree окна.
#
# ПОЧЕМУ НЕ АВТОМАТИЧЕСКИЙ `nova update`. Соблазн велик и он неверен: lock
# существует ради ВОСПРОИЗВОДИМОСТИ. Автоматически перерезолвить пины на каждой
# сборке — значит отменить lock и получать разные сборки в разные дни. Cargo не
# делает этого по той же причине. Правильный инвариант не «пины всегда свежие»,
# а «дерево собирается тем, что записано», — его и проверяем.
#
# ЧЕГО НЕ ЛОВИТ (сказано честно): собирается ОДНА цель, а не весь корпус —
# это проба на резолв зависимостей, а не второй мега-CU. И если сеть недоступна,
# шаг честно сообщает об этом и НЕ красит гейт: отсутствие сети — не дефект
# дерева.
#
# ИСПОЛЬЗОВАНИЕ:
#   bash scripts/guards/check-clean-checkout-build.sh [КОРЕНЬ] [ЦЕЛЬ]
# Самотест — scripts/guards/selftest/test-check-clean-checkout-build.sh

set -u
export LC_ALL=C

ROOT="${1:-$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)}"
TARGET="${2:-examples/flagship/aggregator/src/main.nv}"

cd "$ROOT" || { echo "check-clean-checkout-build: нет каталога $ROOT" >&2; exit 1; }
git rev-parse --git-dir >/dev/null 2>&1 || { echo "check-clean-checkout-build ok: не git-репозиторий"; exit 0; }

NOVA="$ROOT/nova-cli/target/release/nova.exe"
[ -x "$NOVA" ] || NOVA="$ROOT/nova-cli/target/release/nova"
[ -x "$NOVA" ] || { echo "check-clean-checkout-build: нет бинаря $NOVA — собери компилятор" >&2; exit 1; }

# Временное дерево кладём РЯДОМ с репозиторием: правило worktree-location
# (реестр №561) запрещает и `C:`-временные каталоги, и место внутри репы.
WT="$(cd "$ROOT/.." && pwd)/nova-cleanprobe-$$"
BR="cleanprobe-$$"

cleanup() {
    git worktree remove --force "$WT" >/dev/null 2>&1
    git branch -D "$BR" >/dev/null 2>&1
    rm -rf "$WT" >/dev/null 2>&1
}
trap cleanup EXIT

if ! git worktree add -q -b "$BR" "$WT" HEAD >/dev/null 2>&1; then
    echo "check-clean-checkout-build: не удалось завести временное дерево $WT" >&2
    exit 1
fi

# Страховка: если override всё же оказался в дереве — снимаем и говорим об этом.
found=$(find "$WT" -iname "nova.override.toml" -o -iname "nova.local.toml" 2>/dev/null)
if [ -n "$found" ]; then
    echo "check-clean-checkout-build: ВНИМАНИЕ — override попал в чистое дерево (значит он НЕ gitignored):"
    echo "$found" | sed 's/^/    /'
    echo "$found" | while IFS= read -r f; do [ -n "$f" ] && rm -f "$f"; done
fi

# РАНТАЙМ БЕРЁТСЯ ИЗ ГЛАВНОГО ДЕРЕВА — и это не срезание угла, а граница пробы.
#
# `git worktree add` НЕ инициализирует подмодули, поэтому в свежем дереве нет
# `compiler-codegen/nova_rt/libuv`, и сборка падает на `FATAL libuv submodule
# not initialized` ещё до всякой резолюции зависимостей (поймано 2026-08-11
# первым же прогоном шага). Инициализировать подмодуль на каждый прогон — это
# минуты и гигабайты ради того, что мы и не проверяем.
#
# Проверяем мы РЕЗОЛЮЦИЮ ПАКЕТОВ без dev-override: соберётся ли то, что
# записано в `nova.lock.toml`. Рантайм к этому вопросу отношения не имеет и
# берётся у главного дерева — ровно так же, как гейт проверяет наши пакеты
# (`NOVA_STD_PATH`/`NOVA_RT_DIR`/`NOVA_CG_INCLUDE`). Сказано прямо, чтобы никто
# не считал, будто этот шаг проверяет ещё и рантайм: он не проверяет.
LOG="${TMPDIR:-/tmp}/cleanprobe_$$.log"
( cd "$WT" \
  && NOVA_RT_DIR="$ROOT/compiler-codegen/nova_rt" \
     NOVA_CG_INCLUDE="$ROOT/compiler-codegen" \
     "$NOVA" build "$TARGET" --strict-effects ) >"$LOG" 2>&1
rc=$?

if [ "$rc" -eq 0 ]; then
    echo "check-clean-checkout-build ok: $TARGET собирается на чистом дереве"
    rm -f "$LOG"
    exit 0
fi

# Сеть недоступна — не дефект дерева. Отличаем по тексту резолвера.
if grep -qiE "could not resolve host|network is unreachable|failed to connect|timed out" "$LOG"; then
    echo "check-clean-checkout-build: ПРОПУЩЕНО — нет сети для резолва git-зависимостей"
    sed -n '1,5p' "$LOG" | sed 's/^/    /'
    rm -f "$LOG"
    exit 0
fi

echo "check-clean-checkout-build: FAIL — на ЧИСТОМ дереве $TARGET не собирается" >&2
grep -m8 -E "error|ошибк" "$LOG" | sed 's/^/    /' >&2
echo "" >&2
echo "    Это то, что увидит CI, клон и любое окно. Локальный dev-override" >&2
echo "    подменяет пакеты и прячет расхождение: тег пакета не двигает" >&2
echo "    потребителя — перерезолвить может только \`nova update\`" >&2
echo "    (реестр 221.1 №283, №565)." >&2
rm -f "$LOG"
exit 1
