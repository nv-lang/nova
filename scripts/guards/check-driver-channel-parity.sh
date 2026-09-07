#!/bin/sh
# scripts/guards/check-driver-channel-parity.sh — ЧЕТЫРЕ драйвера кодогена кормят
# эмиттер ОДНИМ набором чекер-каналов.
#
# ЧЕТЫРЕ, А НЕ ТРИ (правка 2026-09-07, реестр №1012). Докстрока обещала три
# драйвера, и это была не описка, а слепое пятно: дерево отрастило четвёртый —
# `nova-cli/src/bench/run.rs` со своим конвейером (`parse` →
# `resolve_imports_inline` → `alpha_rename` → `number_exprs` → …) — а страж о
# нём не знал. ЗАМЕР 2026-09-07 С ПРАВИЛОМ СЧЁТА, потому что без него число ничего не значит: ПО ОПРЕДЕЛЕНИЮ ЭТОГО СТРАЖА (каналы, кормящиеся полем `env.<field>`) эталон проводит 8, а бенч-драйвер ОДИН; если же считать ВСЕ вызовы `emitter.set_*` вместе с конфигурационными (`set_bench_mode`, `set_mono_depth_limit`), будет 12 против 4. Значимо первое, и
# `nova bench run` падал на ЛЮБОМ бенче дерева с
# `[E_UNKNOWN_STATIC_METHOD] str.new(...)`, потому что без `resolved_callees`
# вызов резолвился по имени. Страж не «пропустил» — он мерил меньше, чем
# обещал, и это тот же класс, что он сам и сторожит.
#
# План/реестр: docs/plans/221.1-bug-sweep.md №669 (класс Ф.4c: «nova build
# молча пропускал канал»); план 196 (каналы resolved_*), 231.2 §1.
#
# ПРАВИЛО: множество `emitter.set_<channel>(...)` вызовов в
# compiler-codegen/src/test_runner.rs (эталон — nova test) обязано быть
# ПОДМНОЖЕСТВОМ таких вызовов в nova-cli/src/main.rs (nova build) и в
# compiler-codegen/src/main.rs (standalone). Канал, проведённый в test и не
# проведённый в build, — красный: ровно так `nova build` трижды терял каналы
# (Ф.4c: resolved_types/callees; №669: pattern_variant_types,
# resolved_variant_ctors, node_substs) при зелёном `nova test`.
#
# Исключения — только именованные (см. ALLOW ниже) с причиной.
#
# $1 — корень репозитория (default: вычислить от себя).
#
# Проверялся: Windows (Git Bash), 2026-08-15.
export LC_ALL=C
# Корень приводится к АБСОЛЮТНОМУ пути: относительный `.` уводил поиск
# бинаря мимо цели, и страж писал «сломан раннер» о здоровом дереве
# (2026-08-18). Ложная краснота стоит дороже отсутствующей проверки:
# по ней идут искать поломку, которой нет, и в стража перестают верить.
# Если cd не удался — значение СОХРАНЯЕТСЯ как было: пустой ROOT судил бы
# корень файловой системы, а это хуже исходной болезни.
ROOT="${1:-$(dirname "$0")/../..}"
ROOT="$(cd "$ROOT" 2>/dev/null && pwd || printf '%s' "$ROOT")"
NAME=check-driver-channel-parity

TR="$ROOT/compiler-codegen/src/test_runner.rs"
CLI="$ROOT/nova-cli/src/main.rs"
SA="$ROOT/compiler-codegen/src/main.rs"
BENCH="$ROOT/nova-cli/src/bench/run.rs"
for f in "$TR" "$CLI" "$SA" "$BENCH"; do
    [ -f "$f" ] || { echo "$NAME: FAIL — нет $f" >&2; exit 1; }
done

# Только каналы чекера: set_* с аргументом из *_env.<field> — так отсекаем
# сеттеры конфигурации (set_bench_mode, set_source_file_name и т.п.).
chan_set() {
    grep -o 'emitter\.set_[a-z_0-9]*(&[a-z_]*env\.[a-z_0-9]*)' "$1" \
        | sed 's/emitter\.\(set_[a-z_0-9]*\)(.*/\1/' | sort -u
}
# ALLOW: каналы, законно отсутствующие в одном из драйверов (имя причина).
ALLOW=""

TRS=$(chan_set "$TR")
BAD=0
for f in "$CLI" "$SA" "$BENCH"; do
    HAVE=$(chan_set "$f")
    for s in $TRS; do
        case " $ALLOW " in *" $s "*) continue;; esac
        if ! printf '%s\n' "$HAVE" | grep -qx "$s"; then
            echo "$NAME: FAIL — канал $s проведён в test_runner.rs, но не в $(basename "$(dirname "$f")")/$(basename "$f")" >&2
            BAD=1
        fi
    done
done
if [ "$BAD" -ne 0 ]; then
    echo "  Каналы 196 проводятся ВО ВСЕХ ЧЕТЫРЁХ драйверах одной волной (№669, №1012)." >&2
    exit 1
fi
# ─── Проходы конвейера (реестр №1023) ───
#
# Страж сверял КАНАЛЫ и не сверял ПРОХОДЫ — то есть механизм, поставленный держать класс
# «драйвер делает не то же, что эталон», мерил его половину: №1012 (каналы) он бы поймал после
# расширения, а №1020 (пропущенный `self_return_lower`) не поймал бы никогда.
#
# ПРОХОД определяется МАШИНОЙ, а не списком: это вызов, ПРЕОБРАЗУЮЩИЙ модуль —
# `<mod>::<fn>(&mut module`. Перечень берётся из ЭТАЛОНА (`test_runner.rs`), поэтому новый
# проход, добавленный туда, автоматически становится требованием ко всем драйверам.
#
# `tr -d '\r'` ОБЯЗАТЕЛЕН и это не перестраховка: файлы дерева в CRLF, а вызовы бывают
# многострочными (`normalize_chains_module(\n    &mut module, …)`). Без снятия `\r` такой вызов
# не совпадает, и страж требует уже проведённое — поймано на себе 2026-09-07 при заведении.
pass_set() {
    tr -d '\r' < "$1" | tr '\n' ' ' \
        | grep -oE "(crate|nova_codegen)::[a-z_]+::[a-z_0-9]+\([[:space:]]*&mut module" \
        | sed -E 's/^(crate|nova_codegen):://; s/\([[:space:]]*&mut module$//' \
        | sort -u
}

# ALLOW_PASS: «путь-от-корня|проход|причина». Законное отсутствие — только НАЗВАННОЕ, с номером
# строки реестра. Пустая причина не допускается: молчащий ALLOW превращает стража в украшение.
ALLOW_PASS="
nova-cli/src/bench/run.rs|const_fn_eval::rewrite_const_fn_calls|#1023 — не вставлен: нужна проба на каждый проход, вставка по аналогии запрещена
nova-cli/src/bench/run.rs|const_fn_mono::specialize_mixed_const_fns|#1023 — то же
nova-cli/src/bench/run.rs|field_cache::cache_module|#1023 — то же; ТИХО искажает замер, числа прежних бенчей выведены из обращения
nova-cli/src/bench/run.rs|number_exprs::number_unset_exprs|#1023 — бенч зовёт number_exprs; разница по функции не измерена
nova-cli/src/main.rs|const_fn_mono::specialize_mixed_const_fns|#1023 — найдено тем же замером у nova build, НЕ исследовано
"

TRP=$(pass_set "$TR")
PBAD=0
for f in "$CLI" "$SA" "$BENCH"; do
    REL="${f#$ROOT/}"
    HAVE=$(pass_set "$f")
    for s in $TRP; do
        if printf '%s\n' "$ALLOW_PASS" | grep -qF "$REL|$s|"; then continue; fi
        if ! printf '%s\n' "$HAVE" | grep -qx "$s"; then
            echo "$NAME: FAIL — проход $s проведён в test_runner.rs, но не в $REL" >&2
            PBAD=1
        fi
    done
done
if [ "$PBAD" -ne 0 ]; then
    echo "  Проход конвейера, который делает эталон, обязан делать каждый драйвер (№1023)." >&2
    echo "  Законное отсутствие — строкой в ALLOW_PASS с причиной и номером реестра." >&2
    exit 1
fi

N=$(printf '%s\n' "$TRS" | grep -c .)
NP=$(printf '%s\n' "$TRP" | grep -c .)
NA=$(printf '%s\n' "$ALLOW_PASS" | grep -c '|')
echo "$NAME ok: $N чекер-каналов и $NP проходов эталона проведены в nova build, standalone и bench run ($NA законных отсутствия названы в ALLOW_PASS)"
exit 0
