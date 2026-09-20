#!/bin/sh
# Chislo 8/8 napisano RUKOY tochno tak zhe, kak v sluchae A.
# Sluchaev v fayle TRI, a ne vosem - vot oni:
run_case_1; run_case_2; run_case_3
if [ "$FAILED" -eq 0 ]; then echo "test-b ok: 8/8 (koren $ROOT)"; exit 0; fi
