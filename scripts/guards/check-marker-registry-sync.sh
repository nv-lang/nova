#!/bin/sh
# scripts/guards/check-marker-registry-sync.sh
#
# ЗАЧЕМ (план 231 §0а; реестр 221.1 №155 и №161 — урок повторился ДВАЖДЫ за один день).
# Маркер `[M-...]`, поставленный в коде рядом с обходом, но НЕ занесённый ни в один
# реестр, = НЕВИДИМЫЙ ДОЛГ: обход в коде живёт, а дефекта для планирования не
# существует, значит его никто никогда не закроет.
#
# Прецеденты, ради которых страж заведён (оба — 2026-07-30):
#   * A-V8 (витрина): ПЯТЬ маркеров флагмана отсутствовали в реестрах — под ними жили
#     обходы, которые учат читателя плохому (int-коды вместо строковых тегов событий,
#     проверка JSON по подстроке вместо round-trip, `t0.plus(budget)` вместо `t0 + budget`).
#     Заведены записью №155.
#   * Аудит std: ЕЩЁ ТРИ. Запись №161. А первый же прогон ЭТОГО стража нашёл 59 —
#     на порядок больше, чем оба ручных аудита вместе. Ручной обход не масштабируется:
#     только в `std/src` 332 ссылки на маркеры.
#
# ЧТО ПРОВЕРЯЕТ: каждый маркер из `.nv`-исходников обязан встречаться хотя бы в одном
# реестре (`221.1-bug-sweep.md`, `backlog-followups.md`, `simplifications.md`) или в
# каком-либо плане `docs/plans/*.md`.
#
# РЕЖИМ — ХРАПОВИК, НЕ ЗАПРЕТ (как arch-ratchet): на момент заведения долг = 59.
# Страж красный, только если долг ВЫРОС. Снижать baseline можно и нужно — по мере
# заведения записей; поднимать НЕЛЬЗЯ (это и есть смысл храповика).
#
# ЧЕГО НЕ ПРОВЕРЯЕТ (сознательно): обратное направление (маркер в реестре без сайта в
# коде) — это норма: запись может быть заведена до появления обхода либо остаться
# летописью после его снятия.
#
# Самотест: scripts/guards/selftest/test-check-marker-registry-sync.sh
set -u
ROOT="${1:-$(pwd)}"
cd "$ROOT" || exit 2

BASELINE_FILE="scripts/guards/marker-registry.baseline"
REGISTRIES="docs/plans/221.1-bug-sweep.md docs/plans/backlog-followups.md docs/dev/simplifications.md"

markers=$(grep -rhoE "\[M-[A-Za-z0-9._-]+\]" std/src examples spec_tests --include=*.nv 2>/dev/null \
          | sed -e 's/^\[//' -e 's/\]$//' | sort -u)

if [ -z "$markers" ]; then
    echo "check-marker-registry-sync: маркеров в .nv не найдено — нечего сверять"
    exit 0
fi

# Один проход: склеиваем реестры и планы в память, дальше сверяем без пере-чтения.
# СКОРОСТЬ — часть работоспособности. Прежняя редакция клала ВСЮ документацию
# (16 МБ) в переменную оболочки и для КАЖДОГО из ~391 маркера сканировала её
# целиком через `case "$haystack" in *"$m"*)`. Это ~6 ГБ работы в bash и 59
# секунд из 150 всего гейта — замерено профилировщиком `gate-profile.sh` после
# вопроса владельца «что сколько занимает». Здесь ОДИН проход `grep -F -f`:
# шаблоны берутся файлом, документация читается один раз.
PATFILE=$(mktemp) || exit 2
trap 'rm -f "$PATFILE"' EXIT
printf '%s
' $markers > "$PATFILE"

FOUND=$(cat $REGISTRIES docs/plans/*.md 2>/dev/null         | grep -ohFf "$PATFILE" 2>/dev/null | sort -u)

missing=""
n=0
for m in $markers; do
    case "
$FOUND
" in
        *"
$m
"*) : ;;
        *) missing="$missing$m
"; n=$((n + 1)) ;;
    esac
done

baseline=$(grep -E "^unregistered=" "$BASELINE_FILE" 2>/dev/null | sed -e 's/^unregistered=//')
[ -z "$baseline" ] && baseline=0

if [ "$n" -gt "$baseline" ]; then
    echo "MARKER-REGISTRY-SYNC FAIL: неучтённых маркеров $n > baseline $baseline" >&2
    echo "Появился маркер в коде без записи в реестре. Обход в коде живёт, а дефекта" >&2
    echo "для планирования не существует — его никто не закроет." >&2
    echo "Заведи запись в docs/plans/221.1-bug-sweep.md (или backlog-followups.md для" >&2
    echo "follow-up) ТЕМ ЖЕ слиянием, что и обход. Список неучтённых:" >&2
    echo "$missing" | sed -e '/^$/d' -e 's/^/  - /' >&2
    exit 1
fi

if [ "$n" -lt "$baseline" ]; then
    echo "check-marker-registry-sync ok: неучтённых $n (было $baseline) — долг СНИЖЕН,"
    echo "  опусти baseline в $BASELINE_FILE до $n, чтобы храповик зафиксировал прогресс"
    exit 0
fi

echo "check-marker-registry-sync ok: неучтённых $n <= baseline $baseline"
exit 0
