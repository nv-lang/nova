#!/usr/bin/env bash
# scripts/guards/check-doc-truth.sh — нормативная документация врёт: либо
# именем маркера, которого раннер не знает, либо командой `nova`, которая не
# запускается как написана.
#
# ЗАЧЕМ (реестр 221.1 №455). Аудит нашёл `AGENTS.md`, учащий ПРЯМО ОБРАТНОМУ
# действующему правилу — включая маркеры, которых раннер не знает
# (`EXPECT: hello`, `EXPECT_LINT_WARNING`, `SOUNDNESS_REGRESSION`). При
# заведении стража владелец расширил класс: «команды, записанные в
# документации, не исполняются ничем и потому гниют молча». Обе
# документированные формы `nova test` в `AGENTS.md`/`test-conventions.md`
# падали с ПЕРВОГО запуска — `nova test requires at least one path` (Plan
# 172.6 сделал путь обязательным, дока не обновилась НИ РАЗУ с тех пор).
# Страж проверяет ОБЕ оси.
#
# ОСЬ 1 — ИМЕНА МАРКЕРОВ. Каждый токен `EXPECT_[A-Z_]+`, упомянутый в
# `AGENTS.md`, `docs/dev/**/*.md`, `docs/guide/**/*.md`, обязан входить в
# список имён, которые реально разбирает раннер: `parse_expect`
# (`compiler-codegen/src/test_runner.rs` ~:306-332 — семь имён) плюс
# лейн-маркеры `detect_test_type` (~:6280-6292: `EXPECT_TIMEOUT`,
# `EXPECT_EXIT`) плюс отдельно разбираемый бюджет `EXPECT_TIMEOUT_MS`
# (~:2414). Список — константа ниже; при изменении набора имён в раннере
# синхронизировать руками (как в `check-expect-markers.sh`, его код-сиблинг).
#
# ОСЬ 2 — ИСПОЛНИМОСТЬ КОМАНД. Каждая строка вида `nova <sub> ...` /
# `nova-cli/target/release/nova <sub> ...` внутри ``` code-fence в тех же
# файлах проверяется СТАТИЧЕСКИ через `<bin> <sub> --help` (полный прогон НЕ
# нужен и дорог — задание владельца прямым текстом):
#   1) подкоманда существует (`--help` завершается успешно);
#   2) каждый `--флаг` из строки документации есть среди `--флаг`/`--флаг
#      <ЗНАЧЕНИЕ>` в Options-блоке `--help`;
#   3) если Arguments-блок `--help` показывает позиционный аргумент как
#      обязательный (clap-нотация `<ИМЯ>` ЛИБО слово «required»/«at least
#      one» в его описании — Plan 172.6 не отмечает `[PATHS]...` угловыми
#      скобками, обязательность там ТОЛЬКО текстом) — а в строке документации
#      после подкоманды НЕТ ни одного не-флагового токена (флаг-значения
#      распознаются по наличию `<ЗНАЧЕНИЕ>` у флага в `--help`) — FAIL.
# Строки с плейсхолдером (`<...>`, `path/to/`) или с shell-конструкциями
# (`|`, `>`, `;`, продолжение `\`), которые статикой не проверить честно, —
# ПРОПУСКАЮТСЯ, но СЧИТАЮТСЯ и печатаются (иначе плейсхолдер — лазейка).
# ВСЕГДА вызывается только `--help` — реальная команда (тест/бенч/билд)
# никогда не исполняется этим стражем.
#
# ХРАПОВИК (baseline: `doc-truth.baseline`, ключи `unknown_markers` и
# `unrunnable_commands`, только вниз/равно). Не строгий zero-tolerance по
# всему периметру: вне мандата №455 остаётся долг в файлах, которые №455 не
# трогает (`docs/guide/nova-cli(.ru).md`, `docs/dev/bench-conventions.md`,
# `docs/dev/idioms/**`, `docs/dev/migration/**`, `docs/dev/nova-codegen(.ru).md`
# — `docs/guide/**` требует парной ru/en правки через `doc-conventions`,
# отдельное окно). `AGENTS.md` и `docs/dev/test-conventions.md` — мандат
# №455 — обязаны быть чисты по ОБЕИМ осям. Снижение базы приветствуется в
# любом окне.
#
# ИСПОЛЬЗОВАНИЕ: check-doc-truth.sh [корень-репы]
# Бинарь `nova` резолвится как `<корень>/nova-cli/target/release/nova(.exe)`
# (место, куда его кладёт шаг 1 `scripts/gate.sh`). Бинаря нет — ОСЬ 2
# пропускается с предупреждением (не красит гейт из-за окружения), ОСЬ 1
# проверяется всегда.
set -u
export LC_ALL=C
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]:-$0}")" && pwd)"
ROOT="${1:-$(cd "$SCRIPT_DIR/../.." && pwd)}"
BASELINE="$SCRIPT_DIR/doc-truth.baseline"

# ---------- ОСЬ 1: имена EXPECT_*-маркеров ----------

KNOWN='EXPECT_COMPILE_ERROR|EXPECT_CC_ERROR|EXPECT_RUNTIME_PANIC|EXPECT_EXIT_CODE|EXPECT_STDOUT|EXPECT_STDERR|EXPECT_COMPILE_WARNING|EXPECT_TIMEOUT_MS|EXPECT_TIMEOUT|EXPECT_EXIT'

scan_paths=()
[ -f "$ROOT/AGENTS.md" ] && scan_paths+=("$ROOT/AGENTS.md")
[ -d "$ROOT/docs/dev" ] && scan_paths+=("$ROOT/docs/dev")
[ -d "$ROOT/docs/guide" ] && scan_paths+=("$ROOT/docs/guide")

BAD_MARKERS=""
if [ "${#scan_paths[@]}" -gt 0 ]; then
    # -H: force filename prefix even for a single matched file (default grep
    # behaviour omits it for one file — would hide WHICH doc is at fault).
    #
    # Исключение `docs/dev/agent-memory/**` СНЯТО 2026-08-21 вместе с самой
    # выгрузкой: она удалена из репозитория (живёт в аккаунт-каталоге, где ей
    # и место), и периметр возвращён к полному. Сужение, пережившее свою
    # причину, — это слепая зона, на которую никто не соглашался.
    BAD_MARKERS=$(find "${scan_paths[@]}" -type f -name '*.md' -print0 2>/dev/null \
            | xargs -0 grep -HnoE 'EXPECT_[A-Z_]+' 2>/dev/null \
            | grep -vE ":($KNOWN)\$")
fi
unknown_markers=0
[ -n "$BAD_MARKERS" ] && unknown_markers=$(printf '%s\n' "$BAD_MARKERS" | grep -c .)

# ---------- ОСЬ 2: исполнимость команд `nova ...` ----------

BIN=""
for cand in "$ROOT/nova-cli/target/release/nova.exe" "$ROOT/nova-cli/target/release/nova"; do
    [ -x "$cand" ] && BIN="$cand" && break
done

BAD_COMMANDS=""
skipped_commands=0
unrunnable_commands=0

if [ -z "$BIN" ]; then
    echo "check-doc-truth: бинаря $ROOT/nova-cli/target/release/nova(.exe) нет — ОСЬ 2 (исполнимость команд) ПРОПУЩЕНА (собери релиз шагом 1 gate.sh)" >&2
else
    # КЭШ МЕЖДУ ПРОГОНАМИ (2026-08-09). Прежняя редакция брала `mktemp -d` и
    # стирала кэш на выходе — то есть каждый прогон гейта заново дёргал
    # `nova <sub> --help` для всех 25 подкоманд, хотя справка меняется ТОЛЬКО
    # при пересборке бинаря. Профилировщик показал 66с из 142с всего гейта.
    # Ключ кэша — время бинаря: пересобрали компилятор, ключ сменился, кэш
    # построился заново; не пересобирали — прогон почти бесплатен.
    BIN_TS=$(stat -c %Y "$BIN" 2>/dev/null || echo 0)
    CACHE_DIR="${TMPDIR:-/tmp}/nova-doctruth-help-$BIN_TS"
    mkdir -p "$CACHE_DIR" 2>/dev/null

    # ОСЬ 2 сканирует УЖЕ ИНОЙ, УЖЕ пределы (задание владельца): AGENTS.md +
    # docs/dev/** — БЕЗ docs/guide/**. docs/guide/nova-cli(.ru).md — большой
    # CLI-референс с сотнями usage-синтаксис-строк вида
    # `nova build FILE [-o OUTPUT] [...]`; это НЕ примеры вызова (шаблон
    # синтаксиса, не команда), и включение docs/guide/** сюда на порядок
    # раздувает корпус кандидатов без пользы.
    cmd_scan_paths=()
    [ -f "$ROOT/AGENTS.md" ] && cmd_scan_paths+=("$ROOT/AGENTS.md")
    [ -d "$ROOT/docs/dev" ] && cmd_scan_paths+=("$ROOT/docs/dev")

    CANDIDATES=$(find "${cmd_scan_paths[@]}" -type f -name '*.md' -print0 2>/dev/null \
        | xargs -0 awk '
            FNR==1 { infence=0 }
            /^```/  { infence = !infence; next }
            infence && /^(nova |nova-cli\/target\/release\/nova )/ { print FILENAME ":" FNR ":" $0 }
        ')

    # ТЕЛО ЦИКЛА (разбор строки, SKIP-классификация, `nova <sub> --help`,
    # флаг-карта, сверка) вынесено в питон-ядро doc-truth-cmd-scan.py: bash
    # версия форкала sed/grep/printf по 5-10 раз НА КАЖДОГО из ~234
    # кандидатов — доминирующие расходы стража (план 275-Ф.1, профиль
    # показал ~30-40с из ~45с полного прогона). Кандидаты и их порядок —
    # ПРЕЖНИЕ (та же find+awk-связка выше, не тронута: порядок обхода
    # файловой системы — не то, чем стоит рисковать ради скорости).
    PY_TMP="$CACHE_DIR/.doctruth-pyout.$$"
    printf '%s\n' "$CANDIDATES" | python "$SCRIPT_DIR/doc-truth-cmd-scan.py" "$BIN" "$CACHE_DIR" > "$PY_TMP"
    PY_RC=$?
    if [ "$PY_RC" -ne 0 ]; then
        echo "check-doc-truth: FAIL — ядро doc-truth-cmd-scan.py не отработало (rc=$PY_RC)" >&2
        rm -f "$PY_TMP"
        exit 1
    fi

    skipped_commands=$(sed -n 's/^skipped_commands=//p' "$PY_TMP")
    unrunnable_commands=$(sed -n 's/^unrunnable_commands=//p' "$PY_TMP")
    BAD_COMMANDS=$(sed -n 's/^BADCMD://p' "$PY_TMP")
    rm -f "$PY_TMP"
    case "$skipped_commands" in ''|*[!0-9]*) echo "check-doc-truth: FAIL — ядро не вернуло skipped_commands" >&2; exit 1;; esac
    case "$unrunnable_commands" in ''|*[!0-9]*) echo "check-doc-truth: FAIL — ядро не вернуло unrunnable_commands" >&2; exit 1;; esac
fi

# ---------- вердикт (храповик, две метрики) ----------

fail=0
check_ratchet() { # key actual
    local base
    base=$(grep -E "^$1=" "$BASELINE" 2>/dev/null | cut -d= -f2)
    if [ -z "$base" ]; then
        echo "check-doc-truth: нет $1= в $BASELINE" >&2
        fail=1
        return
    fi
    if [ "$2" -gt "$base" ]; then
        echo "check-doc-truth FAIL: $1=$2 > baseline=$base (рост запрещён)" >&2
        fail=1
    else
        echo "check-doc-truth ok: $1=$2 <= baseline=$base"
    fi
}

if [ -n "$BAD_MARKERS" ]; then
    echo "check-doc-truth: НЕИЗВЕСТНЫЕ EXPECT_*-маркеры (раннер их не знает):" >&2
    printf '%s\n' "$BAD_MARKERS" | sed 's/^/    /' >&2
fi
if [ -n "$BAD_COMMANDS" ]; then
    echo "check-doc-truth: КОМАНДЫ, которые не запустятся как написаны:" >&2
    printf '%s\n' "$BAD_COMMANDS" | sed 's/^/    /' >&2
fi
[ -n "$BIN" ] && echo "check-doc-truth: команд из code-fence всего проверено; пропущено (плейсхолдер/shell-конструкция)=$skipped_commands"

check_ratchet unknown_markers "$unknown_markers"
[ -n "$BIN" ] && check_ratchet unrunnable_commands "$unrunnable_commands"

exit $fail
