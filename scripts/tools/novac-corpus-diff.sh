#!/usr/bin/env bash
# scripts/tools/novac-corpus-diff.sh -- the acceptance loop of wave M2b-2 (plan 274.8), one step per call.
#
# METHOD (memory: novac-corpus-c-diff-method): the C of the whole corpus is snapshotted with the
# novac built BEFORE the edit, novac is rebuilt, the corpus is snapshotted again, and `diff -r`
# must be empty (or empty after the step's declared normalisation). The differential guard judges
# behaviour, not text; this loop is what holds the wave's own byte criterion.
#
# ORDER IS NOT OPTIONAL: the "before" snapshot must come from a binary built from the CURRENT
# source before the edit -- after the fast-forward the shell template changed, so the 21:54
# novac.exe embeds an older shell and is NOT a valid baseline for this wave.
#
# EOL: some emitted .c files carry CRLF (they inherit it from their source), so every normaliser
# matches `\r?\n` -- measured 2026-09-08 01:10: the first regex matched `}\n` only and left five
# of six carriers "different" after normalising.
#
# USAGE (always from the worktree root; the shell's cwd drifts between calls):
#   bash scripts/tools/novac-corpus-diff.sh baseline             # build novac from HEAD, snapshot target/c-before
#   bash scripts/tools/novac-corpus-diff.sh after <name>         # build, snapshot target/c-<name>, raw diff vs before
#   bash scripts/tools/novac-corpus-diff.sh after-else <name>    # same, diff after normalising ONLY the else shape
#   bash scripts/tools/novac-corpus-diff.sh after-names <name>   # same, diff after normalising temp names
#   bash scripts/tools/novac-corpus-diff.sh rediff[-else|-names] <name>
#                                                    # no build, no snapshot: re-run the diff on
#                                                    # existing snapshots (normaliser fixes, re-reads)
set -u
T=/d/Sources/nv-lang/nova-p274
cd "$T" || exit 2
NOVA="$T/nova-cli/target/release/nova.exe"
NOVAC="$T/novac/target/novac.exe"
STEP="${1:-}"; NAME="${2:-x}"

build_novac() {
    echo "[m2b2] building novac from $(git -C "$T" log -1 --format=%h) ... ($(date +%H:%M:%S))"
    bash "$T/scripts/tools/with-deadline.sh" 300 "$NOVA" build novac/src/main.nv -o "$NOVAC" > "$T/target/m2b2_build.log" 2>&1
    rc=$?
    echo "[m2b2] build rc=$rc ($(date +%H:%M:%S)); warnings: $(grep -c '^warning' "$T/target/m2b2_build.log")"
    [ $rc -eq 0 ] || { tail -20 "$T/target/m2b2_build.log"; exit 1; }
}

snapshot() {
    out="$1"
    rm -rf "$out"; mkdir -p "$out"
    echo "[m2b2] snapshot -> $out ($(date +%H:%M:%S))"
    bash "$T/scripts/tools/novac-emit-corpus.sh" "$out" > "$out/_emit.log" 2>&1
    # THE DENOMINATOR IS THE REAL EMISSIONS, NOT THE FILES. emit_corpus.sh creates the
    # output file before running novac, so a refused source still leaves a .c -- holding the
    # JSON diagnostic of the refusal, identical in every snapshot. Counting those inflates
    # the corpus a verdict claims to have covered (measured 2026-09-08: 150 files, 94 real).
    _all=$(find "$out" -name '*.c' | wc -l)
    _ref=$(wc -l < "$out/_refused.txt" 2>/dev/null || echo 0)
    echo "[m2b2] snapshot files: $_all; real emissions: $((_all - _ref)); refusal artifacts: $_ref"
}

# Normalisers write a parallel tree so the raw snapshots stay untouched evidence.
normalise_else() {
    src="$1"; dst="$2"; rm -rf "$dst"; mkdir -p "$dst"
    (cd "$src" && find . -name '*.c' -print0) | while IFS= read -r -d '' f; do
        mkdir -p "$dst/$(dirname "$f")"
        # join a line that is only "}" with a following "else", collapsing blanks after it:
        # "}\r\n    else {" -> "} else {" and "}\n    else     if (" -> "} else if (" (the old chained shape)
        perl -0pe 's/\}\r?\n[ \t]*else[ \t]*/} else /g' "$src/$f" > "$dst/$f"
    done
}
normalise_names() {
    # БИЕКЦИЯ, А НЕ СВЁРТКА (замена 2026-09-08, замером на снимках этого же цикла).
    # Прежняя реализация была `sed -E 's/\b_novac_(([a-z_]+_t)|l)[0-9]+\b/T/g'` — каждое имя
    # в ОДИН токен. Она пропускает настоящий дефект: печать, взявшую НЕ ТУ временную в ОДНОМ
    # месте. Проба на `target/c-before`, файл `examples_basics_array_lit_positions.nv.c`,
    # последнее вхождение `_novac_tmp_t2` переписано как `_novac_tmp_t1`: свёртка сказала
    # «идентично», биекция — «различно». Обратная сторона проверена там же: чистый сдвиг
    # нумерации (`t1,t2` -> `t101,t102`), который шаг (3б) разрешает намеренно, для биекции
    # ИДЕНТИЧЕН — она нумерует по порядку ПЕРВОГО появления в файле, а не по самому имени.
    # Обе схемы имён (`_novac_*_tN` и `_novac_lN`) и границы слова сохранены: без границ
    # `some_novac_l7x` совпадал подстрокой, а без второй схемы критерий M2c был бы
    # недостижим НИКОГДА.
    #
    # Питон, а не sed: биекция требует ПАМЯТИ на файл (какое имя уже получило какой токен),
    # а `sed` состояния между совпадениями не держит.
    python "$T/scripts/tools/novac-normalise-temps.py" "$1" "$2"
}

# BASE names the snapshot a step is judged against. It must be the snapshot of the PREVIOUS
# COMMITTED step, not the wave's first baseline: measured 2026-09-08 01:29 -- step 1 was compared
# to `c-before` and showed three files "differing", every hunk being the else-shape change of the
# microslice already committed. A step's diff was reading the previous step's work.
#   BASE=c-else0 bash scripts/tools/novac-corpus-diff.sh after coal1
BASE="${BASE:-c-before}"

do_diff() {
    mode="$1"; name="$2"
    A="$T/target/$BASE"; B="$T/target/c-$name"
    [ -d "$A" ] || { echo "[m2b2] no baseline snapshot target/$BASE"; exit 2; }
    echo "[m2b2] comparing target/$BASE -> target/c-$name"
    [ -d "$B" ] || { echo "[m2b2] no snapshot target/c-$name"; exit 2; }
    case "$mode" in
      else)  normalise_else  "$A" "$T/target/n-base"; normalise_else  "$B" "$T/target/n-$name"; A="$T/target/n-base"; B="$T/target/n-$name" ;;
      names) normalise_names "$A" "$T/target/n-base"; normalise_names "$B" "$T/target/n-$name"; A="$T/target/n-base"; B="$T/target/n-$name" ;;
      raw)   : ;;
    esac
    diff -r -q "$A" "$B" | grep -v '_emit.log' > "$T/target/m2b2_diff_$name.txt"
    n=$(wc -l < "$T/target/m2b2_diff_$name.txt")
    raw_n=$(diff -r -q "$T/target/$BASE" "$T/target/c-$name" | grep -v '_emit.log' | wc -l)
    _ref=$(wc -l < "$T/target/c-$name/_refused.txt" 2>/dev/null || echo 0)
    _real=$(( $(find "$T/target/c-$name" -name '*.c' | wc -l) - _ref ))
    echo "[m2b2] DIFF mode=$mode: $n file(s) differ of $_real real emissions (raw: $raw_n) -> target/m2b2_diff_$name.txt"
    if [ "$n" -eq 0 ]; then
        echo "[m2b2] VERDICT: corpus C identical under normalisation '$mode' (raw differences: $raw_n -- the reverse proof)"
    else
        sed 's/^Files //; s/ and .*//; s#.*/##' "$T/target/m2b2_diff_$name.txt" | head -8
        echo "[m2b2] VERDICT: NOT identical"; exit 1
    fi
}

case "$STEP" in
  baseline)      build_novac; snapshot "$T/target/c-before" ;;
  after)         [ -d "$T/target/$BASE" ] || { echo "[m2b2] run 'baseline' first (or set BASE=)"; exit 2; }; build_novac; snapshot "$T/target/c-$NAME"; do_diff raw   "$NAME" ;;
  after-else)    [ -d "$T/target/$BASE" ] || { echo "[m2b2] run 'baseline' first (or set BASE=)"; exit 2; }; build_novac; snapshot "$T/target/c-$NAME"; do_diff else  "$NAME" ;;
  after-names)   [ -d "$T/target/$BASE" ] || { echo "[m2b2] run 'baseline' first (or set BASE=)"; exit 2; }; build_novac; snapshot "$T/target/c-$NAME"; do_diff names "$NAME" ;;
  rediff)        do_diff raw   "$NAME" ;;
  rediff-else)   do_diff else  "$NAME" ;;
  rediff-names)  do_diff names "$NAME" ;;
  *) echo "usage: $0 baseline | after[-else|-names] <name> | rediff[-else|-names] <name>"; exit 2 ;;
esac
