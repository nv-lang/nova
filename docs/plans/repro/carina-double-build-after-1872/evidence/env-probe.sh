#!/bin/bash
set -eu
S="$(cd "$(dirname "$0")" && pwd)"
export TMPDIR="$S" TEMP="$(cygpath -w "$S")" TMP="$(cygpath -w "$S")"
unset BASH_XTRACEFD GC_DONT_GC DOUBLE_BUILD_A
export PYTHONIOENCODING=utf-8
export LC_ALL=C
bash "$S/scripts/tools/with-deadline.sh" 60 python "$S/env-probe.py" env-probe-deadline > "$S/env-probe-deadline.log" 2>&1
cat "$S/env-probe-deadline.log"
