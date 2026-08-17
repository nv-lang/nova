#!/bin/sh
# scripts/guards/check-driver-channel-parity.sh — три драйвера кодогена кормят
# эмиттер ОДНИМ набором чекер-каналов.
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
for f in "$TR" "$CLI" "$SA"; do
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
for f in "$CLI" "$SA"; do
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
    echo "  Каналы 196 проводятся ВО ВСЕХ трёх драйверах одной волной (№669)." >&2
    exit 1
fi
N=$(printf '%s\n' "$TRS" | grep -c .)
echo "$NAME ok: $N чекер-каналов test_runner проведены и в nova build, и в standalone"
exit 0
