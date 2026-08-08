#!/usr/bin/env bash
# check-single-mco-resume.sh — машинный страж: единственный вызов mco_resume
# во всём рантайме Vela — внутри fibers.h::nova_resume_fiber.
#
# ПОЧЕМУ (реестр 221.1 №446/№447, ревизия плана 250, окно presume-cas-gate,
# 2026-08-08). ДО унификации четыре resume-сайта в рантайме (главный цикл
# _worker_main, cleanup-дренаж того же _worker_main, _worker_run_one_fiber
# — все runtime.c, и nova_supervised_step — fibers.h) каждый открытым кодом
# повторяли «restore TLS → гейт CAS IDLE→RUNNING → mco_resume → пост-resume
# классификация». ДВА из четырёх копий несли ОДИН И ТОТ ЖЕ живой дефект
# (№446): переменная владения по умолчанию была `true` ВНЕ ветки
# `MCO_SUSPENDED`, так что дубликат уже мёртвого `co` (источник — гонка
# `wake_pending` duplicate-push, задокументированная самим кодом) пропускал
# CAS целиком и всё равно доходил до ВТОРОГО `mco_destroy` + ВТОРОГО
# `nova_scope_sweep_dead_child` — двойной free и уход `pending_sweeps` в
# минус. Фикс — ОДНА функция `nova_resume_fiber` (restore → CAS-гейт →
# resume → restore → классификация), единственный вызов `mco_resume` в
# рантайме; инвариант «ни одно действие над `co` не выполняется вне
# выигранного CAS» верен ПО ПОСТРОЕНИЮ, а не по соглашению на N сайтах —
# ровно тот класс, который mn-coding-conventions.md §0.5 требует закрывать
# консолидацией состояния, а не добавлением очередного охранника.
#
# ЧТО ПРОВЕРЯЕТ. Грепает КАЖДЫЙ вызов `mco_resume(` в `compiler-codegen/
# nova_rt/**/*.c` и `**/*.h` и требует, чтобы вне тела
# `nova_resume_fiber` (fibers.h) их было РОВНО НОЛЬ, за вычетом ДВУХ
# документированных исключений:
#   1. `minicoro.h` — вендоренная третьесторонняя библиотека, которая САМА
#      ОПРЕДЕЛЯЕТ `mco_resume` (это не вызов, это определение того, что
#      оборачивает наш единственный вызов) — вне периметра проверки целиком.
#   2. `nova_fiber_run` (fibers.h) — легаси one-shot путь БЕЗ CAS-гейта
#      вообще (см. его собственный комментарий «Plan 83.4.5.7: no CAS guard
#      здесь — nova_fiber_run is one-shot, single thread, no concurrent
#      resume race»): его `user_data` НЕ является `NovaSpawnCtxBase*`
#      (единственный вызывающий — `test_fibers_deep.c`, свой произвольный
#      struct), маршрутизация через `nova_resume_fiber` прочитала бы
#      `_nova_fiber_state` по чужому смещению памяти — небезопасно и вне
#      темы №446/№447 (там нет ни планировщика, ни конкурентного resume).
#      Явный allowlist ОДНОЙ строки, а не тихое исключение файла.
#   3. `test_*.c` (test_fibers_deep.c, test_runq.c, test_gc_deep.c) —
#      автономные ручные unit-тесты миникоро-примитивов НАПРЯМУЮ (не через
#      Vela-планировщик вообще — сравни `docs/dev/mn-coding-conventions.md`
#      §0.5), собираются отдельно (`cl`/`clang` вручную, не через `nova
#      build`), вне периметра гейта по построению.
#
# Комментарии-упоминания `mco_resume(...)` внутри блочных `/* ... */`
# (строки, начинающиеся с `*` после отступа) НЕ считаются вызовами —
# отфильтрованы явно (иначе страж ловил бы собственную документацию).
#
# ИСПОЛЬЗОВАНИЕ: scripts/guards/check-single-mco-resume.sh [корень-репы]
# Без аргумента — своя репа; аргумент — для самотеста (фикстуры).
# Коды: 0 — единственный вызов на месте, 1 — найден посторонний mco_resume.
#
# План: docs/plans/221.1-bug-sweep.md №446/№447; docs/dev/mn-coding-
# conventions.md §0.5 (одно авторитетное слово состояния), §10/§11.

set -euo pipefail
export LC_ALL=C   # байтовый grep — см. lesson в других стражах этой папки

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]:-$0}")" && pwd)"
REPO_ROOT="${1:-$(cd "$SCRIPT_DIR/../.." && pwd)}"
RT_DIR="$REPO_ROOT/compiler-codegen/nova_rt"

if [ ! -d "$RT_DIR" ]; then
    echo "check-single-mco-resume: $RT_DIR не найден — нечего проверять"
    exit 0
fi

# Реальные вызовы mco_resume( в файле — с полноценным (stateful) вычитанием
# комментариев /* ... */ (в т.ч. многострочных и открывающихся не с '*') и
# // построчных, а не эвристикой по первому символу строки (та ловит только
# ПРОДОЛЖЕНИЯ блочных комментариев, не их ОТКРЫВАЮЩУЮ строку — поймано
# собственным самотестом стража при первой редакции).
_real_call_lines() {
    local f="$1"
    awk '
        {
            line = $0
            if (in_comment) {
                idx = index(line, "*/")
                if (idx == 0) { next }
                line = substr(line, idx + 2)
                in_comment = 0
            }
            out = ""
            rest = line
            while (1) {
                si = index(rest, "/*")
                ci = index(rest, "//")
                if (si == 0 && ci == 0) { out = out rest; break }
                if (ci > 0 && (si == 0 || ci < si)) { out = out substr(rest, 1, ci - 1); break }
                out = out substr(rest, 1, si - 1)
                tail = substr(rest, si + 2)
                ei = index(tail, "*/")
                if (ei == 0) { in_comment = 1; rest = ""; break }
                rest = substr(tail, ei + 2)
            }
            if (out ~ /mco_resume\(/) print NR
        }
    ' "$f"
}

problems=0

while IFS= read -r -d '' f; do
    base="$(basename "$f")"
    [ "$base" = "minicoro.h" ] && continue          # allowlist 1: третья сторона
    case "$base" in
        test_*.c) continue ;;                        # allowlist 3: standalone unit-тесты
    esac

    lines="$(_real_call_lines "$f")"
    [ -z "$lines" ] && continue

    if [ "$base" = "fibers.h" ]; then
        # Найти диапазон тела nova_resume_fiber: со строки сигнатуры считать
        # баланс { и } до возврата к нулю (конец функции).
        body_start="$(grep -n 'static inline NovaResumeOutcome nova_resume_fiber(' "$f" | head -1 | cut -d: -f1)"
        if [ -z "$body_start" ]; then
            echo "  ✗ fibers.h: не нашёл определение nova_resume_fiber — страж не может определить периметр" >&2
            problems=$((problems + 1))
            continue
        fi
        body_end="$(awk -v s="$body_start" '
            NR < s { next }
            {
                n = gsub(/\{/, "{"); m = gsub(/\}/, "}")
                depth += n - m
                if (NR >= s && depth == 0 && seen_open) { print NR; exit }
                if (n > 0) seen_open = 1
            }
        ' "$f")"
        if [ -z "$body_end" ]; then
            echo "  ✗ fibers.h: не нашёл конец тела nova_resume_fiber — страж не может определить периметр" >&2
            problems=$((problems + 1))
            continue
        fi

        # nova_fiber_run allowlist: ровно одна строка (allowlist 2).
        fiber_run_line="$(grep -n 'static inline void nova_fiber_run(' "$f" | head -1 | cut -d: -f1)"

        while IFS= read -r ln; do
            [ -z "$ln" ] && continue
            if [ "$ln" -ge "$body_start" ] && [ "$ln" -le "$body_end" ]; then
                continue   # внутри тела nova_resume_fiber — законно
            fi
            # allowlist 2: единственная строка внутри nova_fiber_run (ищем
            # локально — следующая функция после её сигнатуры, до 25 строк).
            if [ -n "$fiber_run_line" ] && [ "$ln" -ge "$fiber_run_line" ] && [ "$ln" -le "$((fiber_run_line + 25))" ]; then
                content="$(sed -n "${ln}p" "$f")"
                if echo "$content" | grep -q 'r = mco_resume(co);'; then
                    continue   # nova_fiber_run's единственный законный вызов
                fi
            fi
            echo "  ✗ $base:$ln — mco_resume вне nova_resume_fiber и вне allowlist nova_fiber_run" >&2
            problems=$((problems + 1))
        done <<< "$lines"
        continue
    fi

    # Любой другой файл periметра (runtime.c и т.д.) — ЛЮБОЙ реальный вызов
    # mco_resume( — нарушение (единственный источник — fibers.h).
    while IFS= read -r ln; do
        [ -z "$ln" ] && continue
        echo "  ✗ $base:$ln — mco_resume вне fibers.h::nova_resume_fiber" >&2
        problems=$((problems + 1))
    done <<< "$lines"
done < <(find "$RT_DIR" -maxdepth 1 \( -name '*.c' -o -name '*.h' \) -print0)

if [ "$problems" -ne 0 ]; then
    cat >&2 <<'HINT'

Найден вызов mco_resume() вне единственного авторитетного места
(fibers.h::nova_resume_fiber). Инвариант 221.1 №446/№447: «ни одно
действие над co (resume/destroy/sweep) не выполняется вне выигранного
CAS» держится ТОЛЬКО если resume идёт через ОДНУ функцию. Новый resume-
сайт обязан звать nova_resume_fiber(co, tls_ctx, restore_inner,
save_inner), а не открывать свой mco_resume().
HINT
    exit 1
fi

echo "check-single-mco-resume ok: mco_resume только внутри nova_resume_fiber (+ документированные allowlist'ы)"
