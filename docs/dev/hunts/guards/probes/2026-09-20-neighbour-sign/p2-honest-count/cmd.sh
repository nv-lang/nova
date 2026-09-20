#!/usr/bin/env bash
# Проба к находке 2: check-selftest-honest-count.py считает законным ЛЮБУЮ
# строку вердикта, в которой встретился знак `$`, — вместо того чтобы судить,
# ВЫЧИСЛЕНО ли число.
#
# Запуск:  bash cmd.sh <КОРЕНЬ-РЕПОЗИТОРИЯ>
set -u
REPO="${1:?ukazhi koren repozitoriya nova pervym argumentom}"
GUARD="$REPO/scripts/guards/check-selftest-honest-count.py"
[ -f "$GUARD" ] || { echo "net $GUARD" >&2; exit 2; }

HERE="$(cd "$(dirname "$0")" && pwd)"
BASE="$HERE/base.baseline"
printf 'literal=0\n' > "$BASE"
export NOVAC_SELFTEST_COUNT_BASELINE="$BASE"

# --- A: golaya rukopisnaya stroka - strazh lovit (kontrol) -------------
A="$HERE/case-a"; rm -rf "$A"; mkdir -p "$A"
cat > "$A/test-a.sh" <<'SH'
#!/bin/sh
if [ "$FAILED" -eq 0 ]; then echo "test-a ok: 8/8"; exit 0; fi
SH
echo "=== A. ruka bez znaka dollara -> ozhidaem KRASNYY"
python "$GUARD" "$REPO" "$A"; echo "rc=$?"

# --- B: TA ZHE rukopisnaya 8/8, no v stroke est $ ----------------------
B="$HERE/case-b"; rm -rf "$B"; mkdir -p "$B"
cat > "$B/test-b.sh" <<'SH'
#!/bin/sh
# Chislo 8/8 napisano RUKOY tochno tak zhe, kak v sluchae A.
# Sluchaev v fayle TRI, a ne vosem - vot oni:
run_case_1; run_case_2; run_case_3
if [ "$FAILED" -eq 0 ]; then echo "test-b ok: 8/8 (koren $ROOT)"; exit 0; fi
SH
echo
echo "=== B. ta zhe ruka 8/8 + lyuboy \$ v stroke -> strazh ZELEN"
python "$GUARD" "$REPO" "$B"; echo "rc=$?"

# --- C: eshche deshevle - $ v kommentarii toy zhe stroki ---------------
C="$HERE/case-c"; rm -rf "$C"; mkdir -p "$C"
cat > "$C/test-c.sh" <<'SH'
#!/bin/sh
if [ "$FAILED" -eq 0 ]; then echo "test-c ok: 8/8 $"; exit 0; fi
SH
echo
echo "=== C. odin golyy simvol \$ vnutri kavychek -> strazh ZELEN"
python "$GUARD" "$REPO" "$C"; echo "rc=$?"
exit 0
