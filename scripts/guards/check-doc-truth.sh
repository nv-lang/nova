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
    CACHE_DIR=$(mktemp -d)
    trap 'rm -rf "$CACHE_DIR"' EXIT

    help_for() { # subcommand -> stdout: help text; сохраняет exit-код в parallel-файл
        local sub="$1" key
        key="${sub//[^a-zA-Z0-9]/_}"
        local hf="$CACHE_DIR/$key.help" xf="$CACHE_DIR/$key.exit"
        if [ ! -f "$hf" ]; then
            "$BIN" "$sub" --help > "$hf" 2>&1
            echo "$?" > "$xf"
        fi
        cat "$hf"
    }
    exit_for() {
        local sub="$1" key
        key="${sub//[^a-zA-Z0-9]/_}"
        cat "$CACHE_DIR/$key.exit" 2>/dev/null || echo 1
    }

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

    while IFS= read -r cand; do
        [ -z "$cand" ] && continue
        file="${cand%%:*}"
        rest="${cand#*:}"
        lineno="${rest%%:*}"
        line="${rest#*:}"
        stripped=$(printf '%s' "$line" | sed -E 's/[[:space:]]+#.*$//')

        if printf '%s' "$stripped" | grep -qE '<[^>]+>|path/to/'; then
            skipped_commands=$((skipped_commands + 1))
            echo "SKIP(placeholder) $file:$lineno: $line" >&2
            continue
        fi
        if printf '%s' "$stripped" | grep -qE '[|;]|>|\\$'; then
            skipped_commands=$((skipped_commands + 1))
            echo "SKIP(shell-construct) $file:$lineno: $line" >&2
            continue
        fi

        read -ra tok <<< "$stripped"
        sub="${tok[1]:-}"
        if [ -z "$sub" ]; then
            skipped_commands=$((skipped_commands + 1))
            echo "SKIP(no-subcommand) $file:$lineno: $line" >&2
            continue
        fi

        help_for "$sub" >/dev/null   # populates .help/.exit cache on first sight of $sub
        if [ "$(exit_for "$sub")" != "0" ]; then
            unrunnable_commands=$((unrunnable_commands + 1))
            BAD_COMMANDS="${BAD_COMMANDS}${file}:${lineno}: unknown-subcommand '${sub}' -- ${line}\n"
            continue
        fi

        # Флаг-карта и требуемость позиционника — считаются ОДИН РАЗ на
        # подкоманду и кэшируются (иначе N команд одной подкоманды пересчитывают
        # одно и то же — доминирующие расходы при большом корпусе доков).
        key="${sub//[^a-zA-Z0-9]/_}"
        af_f="$CACHE_DIR/$key.allflags" vf_f="$CACHE_DIR/$key.valflags" req_f="$CACHE_DIR/$key.required"
        if [ ! -f "$af_f" ]; then
            HELP=$(help_for "$sub")
            printf '%s\n' "$HELP" | grep -oE '^ {2,}--[a-zA-Z][a-zA-Z0-9-]* <[A-Z_]+>' | sed -E 's/^ *(--[a-zA-Z0-9-]+).*/\1/' | sort -u > "$vf_f"
            BOOL_FLAGS=$(printf '%s\n' "$HELP" | grep -oE '^ {2,}--[a-zA-Z][a-zA-Z0-9-]*$' | sed -E 's/^ *//' | sort -u)
            printf '%s\n%s\n' "$(cat "$vf_f")" "$BOOL_FLAGS" | grep -v '^$' | sort -u > "$af_f"
            ARGS_BLOCK=$(printf '%s\n' "$HELP" | awk '/^Arguments:/{f=1;next}/^Options:/{f=0}f')
            req=0
            printf '%s\n' "$ARGS_BLOCK" | grep -qE '^ *<[A-Za-z_]+>' && req=1
            printf '%s\n' "$ARGS_BLOCK" | grep -qi 'required\|at least one' && req=1
            echo "$req" > "$req_f"
        fi
        VALUE_FLAGS=$(cat "$vf_f")
        ALL_FLAGS=$(cat "$af_f")
        required=$(cat "$req_f")

        problem=""
        has_positional=0
        skip_next=0
        for ((i = 2; i < ${#tok[@]}; i++)); do
            t="${tok[$i]}"
            if [ "$skip_next" = "1" ]; then
                skip_next=0
                continue
            fi
            if [[ "$t" == --* ]]; then
                fname="${t%%=*}"
                if ! printf '%s\n' "$ALL_FLAGS" | grep -qxF -- "$fname"; then
                    problem="${problem}unknown-flag(${fname}) "
                elif printf '%s\n' "$VALUE_FLAGS" | grep -qxF -- "$fname" && [[ "$t" != *=* ]]; then
                    skip_next=1
                fi
            else
                has_positional=1
            fi
        done

        if [ "$required" = "1" ] && [ "$has_positional" = "0" ]; then
            problem="${problem}missing-required-positional "
        fi

        if [ -n "$problem" ]; then
            unrunnable_commands=$((unrunnable_commands + 1))
            BAD_COMMANDS="${BAD_COMMANDS}${file}:${lineno}: ${problem}-- ${line}\n"
        fi
    done <<< "$CANDIDATES"
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
    printf '%b' "$BAD_COMMANDS" | sed 's/^/    /' >&2
fi
[ -n "$BIN" ] && echo "check-doc-truth: команд из code-fence всего проверено; пропущено (плейсхолдер/shell-конструкция)=$skipped_commands"

check_ratchet unknown_markers "$unknown_markers"
[ -n "$BIN" ] && check_ratchet unrunnable_commands "$unrunnable_commands"

exit $fail
