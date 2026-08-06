#!/usr/bin/env bash
# check-build-test-identity.sh — приёмочный инструмент: доказывает, что
# `nova build` (nova-cli/src/main.rs::cmd_build) и `nova test-build`
# (compiler-codegen/src/test_runner.rs::run_one) порождают ОДИНАКОВЫЙ C для
# одного и того же исходника.
#
# ЗАЧЕМ. `cmd_build` (путь пользователя) и `test_runner.rs` (путь тестов) —
# ДВЕ независимые, вручную поддерживаемые копии одного 19-шагового
# конвейера (resolve-imports → ... → codegen). За историю проекта — ПЯТЬ
# независимых инцидентов класса «cmd_build забыл вызов, который есть у
# test_runner»; три видны прямо в комментариях main.rs (A-Q3 и др.), два
# найдены окном p-build304 (2026-08-04, реестр 221.1 №304): `cmd_build`
# никогда не звал `field_cache::cache_module` (план 123 не применялся к
# пользовательским сборкам НИ РАЗУ) и никогда не звал
# `emitter.set_source_file_name` (паника из `nova build` печатала
# `<unknown>:N` вместо имени файла). Оба нашлись ОДНИМ методом: собрать
# один и тот же исходник обоими путями и построчно сравнить `.c`
# (см. docs/plans/wip/PROGRESS-build304.md в истории, коммит ac684356f, и
# слитый коммит 54abcdcab). Этот скрипт — тот же метод, оформленный как
# повторяемый инструмент.
#
# КАК. Для каждой фикстуры:
#   1. Копирует ЕДИНСТВЕННЫЙ файл в ДВА ИЗОЛИРОВАННЫХ каталога (ловушка,
#      пойманная окном p-build304: модель модулей Nova — «папка = ОДИН
#      модуль из co-equal файлов» — если положить пробу рядом с другими
#      .nv, компилятор утянет их как часть того же модуля; сравнение
#      станет мусорным).
#   2. Собирает build-сторону: `nova build <file> --keep-artifacts` с
#      TEMP/TMP, указанными на свежий пустой каталог — `.c` build-пути
#      пишется в `%TEMP%/nova_tests-<pid>/build-<hash>/<stem>.c`
#      (nova-cli/src/main.rs, `default_tmp_dir`/`path_hash`); указывая
#      TEMP на пустой каталог, `.c` находится однозначным `find`, без
#      воспроизведения хэш-функции.
#   3. Собирает test-сторону: `nova test-build <file> --keep-artifacts` —
#      `.c` пишется РЯДОМ с исходником (`opts.nv_file.with_extension("c")`,
#      test_runner.rs) — путь известен заранее.
#   4. Сравнивает оба `.c` через check-build-test-identity.py: короткий
#      список известных легитимных расхождений (см. КАНОН НИЖЕ и сам .py)
#      применяется, ПОСЛЕ чего сравнение обязано дать точное совпадение;
#      расхождение печатается с именами функций, чьи тела разошлись, и
#      первыми строками diff.
#
# ИЗВЕСТНОЕ ЛЕГИТИМНОЕ РАСХОЖДЕНИЕ (см. check-build-test-identity.py,
# KNOWN_EXCEPTIONS — это КАНОНИЧЕСКИЙ источник, список здесь — для
# читателя): функция `nova_fn_7runtime7fmt_buf7scratch` эмитируется ТОЛЬКО
# test-путём, у неё НОЛЬ мест вызова в ОБОИХ выводах (DCE/reachability-
# регистрация compiler-codegen, не семантика; найдено окном p-build304).
# Из-за неё все последующие синтетические temp-имена (`_nv_tmp_N` и т.п.)
# в test-.c сдвинуты на константу — это КАНОНИЗИРУЕТСЯ (переименование по
# порядку первого появления), не считается расхождением.
#
# ИСПОЛЬЗОВАНИЕ:
#   scripts/tools/check-build-test-identity.sh [--keep] [FILE.nv ...]
#       Полный прогон: сборка обеими сторонами + сравнение. Без аргументов
#       — фикстуры по умолчанию (bench/field_cache/*.nv — уже используются
#       окном p-build304, самодостаточны, с fn main()). Путь фикстуры может
#       быть абсолютным или относительным корню репо. --keep — не удалять
#       рабочий каталog (печатается путь) — для отладки расхождения.
#   scripts/tools/check-build-test-identity.sh --compare A.c B.c
#       Только сравнение двух готовых .c (без сборки) — используется
#       самотестом scripts/guards/selftest/test-check-build-test-identity.sh.
#
# ПЕРЕМЕННЫЕ ОКРУЖЕНИЯ:
#   NOVA_BIN            путь к собранному `nova`/`nova.exe`; по умолчанию
#                        <repo>/nova-cli/target/release/nova(.exe).
#   NOVA_GC_LIB_DIR / NOVA_GC_INCLUDE_DIR / VCPKG_ROOT
#                        те же штатные переменные, что и у `nova build`
#                        (nova-cli --help); нужны, когда в РЕПО, из которой
#                        запускается скрипт, нет своего
#                        compiler-codegen/vcpkg_installed (типично для
#                        свежего worktree) — тогда указать на главную репу,
#                        как для любой обычной сборки вне главной репы (см.
#                        scripts/guards/check-no-runtime-copy.sh).
#
# ВЫХОД: 0 — все фикстуры идентичны (с точностью до известных исключений);
# 1 — реальное расхождение хотя бы в одной фикстуре, ЛИБО `nova build`/
# `nova test-build` упали сами по себе; 2 — ошибка использования/окружения
# (не найден `nova`/python, файл фикстуры не существует и т.п.).
#
# НЕ в gate.sh (решение владельца 2026-08-04): требует ДВУХ полных сборок
# на каждую фикстуру — дорого для гейта, который гоняется многократно за
# день. Когда гонять — см. docs/dev/test-conventions.md, раздел про этот
# скрипт.

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]:-$0}")" && pwd)"
# Скрипт живёт в scripts/tools/ — корень репы на два уровня выше.
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
COMPARATOR="$SCRIPT_DIR/check-build-test-identity.py"

DEFAULT_FIXTURES=(
    "bench/field_cache/01_ro_hot_loop.nv"
    "bench/field_cache/02_chain_heavy.nv"
)

err() { echo "check-build-test-identity: $*" >&2; }

# --- найти python (см. scripts/tools/demojibake.py и соседей — `python`
# в этом окружении рабочий, `python3` на Windows нередко alias-заглушка
# Microsoft Store) ---
find_py() {
    for cand in python python3; do
        if command -v "$cand" >/dev/null 2>&1 && "$cand" --version >/dev/null 2>&1; then
            echo "$cand"
            return 0
        fi
    done
    return 1
}
PY="$(find_py)" || {
    err "не найден рабочий python (нужен для check-build-test-identity.py)"
    exit 2
}

# --- найти собранный nova ---
find_nova_bin() {
    if [ -n "${NOVA_BIN:-}" ]; then
        echo "$NOVA_BIN"
        return 0
    fi
    for cand in \
        "$REPO_ROOT/nova-cli/target/release/nova.exe" \
        "$REPO_ROOT/nova-cli/target/release/nova"; do
        if [ -x "$cand" ] || [ -f "$cand" ]; then
            echo "$cand"
            return 0
        fi
    done
    return 1
}
NOVA_BIN_RESOLVED="$(find_nova_bin)" || {
    err "не найден собранный nova (искал \$NOVA_BIN, <repo>/nova-cli/target/release/nova[.exe])"
    err "собрать: (cd \"$REPO_ROOT/nova-cli\" && cargo build --release)"
    exit 2
}

# --- режим --compare A B: только сравнение, без сборки (самотест) ---
if [ "${1:-}" = "--compare" ]; then
    if [ "$#" -ne 3 ]; then
        err "--compare требует ровно два пути: --compare A.c B.c"
        exit 2
    fi
    exec "$PY" "$COMPARATOR" "$2" "$3"
fi

# --- полный прогон: разобрать --keep, собрать список фикстур ---
keep=0
if [ "${1:-}" = "--keep" ]; then
    keep=1
    shift
fi

fixtures=()
if [ "$#" -gt 0 ]; then
    fixtures=("$@")
else
    fixtures=("${DEFAULT_FIXTURES[@]}")
fi

resolved_fixtures=()
for f in "${fixtures[@]}"; do
    if [ -f "$f" ]; then
        resolved_fixtures+=("$(cd "$(dirname "$f")" && pwd)/$(basename "$f")")
    elif [ -f "$REPO_ROOT/$f" ]; then
        resolved_fixtures+=("$REPO_ROOT/$f")
    else
        err "фикстура не найдена: $f"
        exit 2
    fi
done

WORK_ROOT="$(mktemp -d)"
cleanup() {
    if [ "$keep" -eq 1 ]; then
        echo "check-build-test-identity: --keep — рабочий каталог сохранён: $WORK_ROOT"
    else
        rm -rf "$WORK_ROOT"
    fi
}
trap cleanup EXIT

echo "check-build-test-identity: nova = $NOVA_BIN_RESOLVED"
echo "check-build-test-identity: python = $PY"
echo "check-build-test-identity: рабочий каталог = $WORK_ROOT"
echo

overall=0
declare -a summary_lines=()

for fpath in "${resolved_fixtures[@]}"; do
    name="$(basename "$fpath" .nv)"
    echo "=== $name ($fpath) ==="

    bdir="$WORK_ROOT/build_side/$name"
    tdir="$WORK_ROOT/test_side/$name"
    tmproot="$WORK_ROOT/build_tmp/$name"
    mkdir -p "$bdir" "$tdir" "$tmproot"
    cp "$fpath" "$bdir/$name.nv"
    cp "$fpath" "$tdir/$name.nv"

    build_log="$WORK_ROOT/build_$name.log"
    test_log="$WORK_ROOT/test_$name.log"

    ( cd "$REPO_ROOT" && TEMP="$tmproot" TMP="$tmproot" \
        "$NOVA_BIN_RESOLVED" build "$bdir/$name.nv" --keep-artifacts \
        -o "$bdir/$name.exe" ) >"$build_log" 2>&1
    build_rc=$?

    ( cd "$REPO_ROOT" && \
        "$NOVA_BIN_RESOLVED" test-build "$tdir/$name.nv" --keep-artifacts \
    ) >"$test_log" 2>&1
    test_rc=$?

    if [ "$build_rc" -ne 0 ]; then
        echo "  FAIL: nova build упал (exit=$build_rc), хвост лога:"
        tail -n 15 "$build_log" | sed 's/^/    /'
        summary_lines+=("$name: FAIL (nova build упал)")
        overall=1
        echo
        continue
    fi
    if [ "$test_rc" -ne 0 ]; then
        echo "  FAIL: nova test-build упал (exit=$test_rc), хвост лога:"
        tail -n 15 "$test_log" | sed 's/^/    /'
        summary_lines+=("$name: FAIL (nova test-build упал)")
        overall=1
        echo
        continue
    fi

    mapfile -t build_c_candidates < <(find "$tmproot" -iname "*.c" 2>/dev/null)
    if [ "${#build_c_candidates[@]}" -ne 1 ]; then
        echo "  ERROR: ожидался ровно один .c от build-стороны под $tmproot, найдено ${#build_c_candidates[@]}:"
        printf '    %s\n' "${build_c_candidates[@]}"
        summary_lines+=("$name: ERROR (не найден build .c однозначно)")
        overall=1
        echo
        continue
    fi
    build_c="${build_c_candidates[0]}"
    test_c="$tdir/$name.c"
    if [ ! -f "$test_c" ]; then
        echo "  ERROR: не найден test-build .c: $test_c"
        summary_lines+=("$name: ERROR (не найден test .c)")
        overall=1
        echo
        continue
    fi

    if "$PY" "$COMPARATOR" "$build_c" "$test_c"; then
        summary_lines+=("$name: PASS (build и test-build породили идентичный C)")
    else
        summary_lines+=("$name: FAIL (build и test-build породили РАЗНЫЙ C — см. вывод выше)")
        overall=1
    fi
    echo
done

echo "=== итог ==="
for line in "${summary_lines[@]}"; do
    echo "  $line"
done
if [ "$overall" -eq 0 ]; then
    echo "check-build-test-identity: PASS — build и test-build идентичны на всех ${#resolved_fixtures[@]} фикстур(ах)"
else
    echo "check-build-test-identity: FAIL — расхождение (см. выше); красный = build и test-build разошлись в конвейере, чинить в компиляторе (не здесь)"
fi
exit "$overall"
