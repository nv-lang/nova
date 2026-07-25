#!/bin/sh
# scripts/arch-ratchet.sh — архитектурный храповик компилятора (план 231 трек Е).
# Метрики НЕ МОГУТ РАСТИ относительно baseline (снижение — приветствуется, обнови baseline).
# Осознанное исключение = правка scripts/arch-ratchet.baseline В ТОМ ЖЕ коммите (видно в ревью).
set -u
BASE_FILE="$(dirname "$0")/arch-ratchet.baseline"
EMIT="compiler-codegen/src/codegen/emit_c.rs"

m_lines=$(wc -l < "$EMIT" | tr -d ' ')
m_infer=$(grep -c "infer_expr_c_type" "$EMIT")
fail=0
while IFS='=' read -r key base; do
  case "$key" in \#*|'') continue;; esac
  cur=$(eval echo "\$m_$key")
  if [ "$cur" -gt "$base" ]; then
    echo "ARCH-RATCHET FAIL: $key=$cur > baseline=$base (emit_c must not grow; fix belongs to checker channel / IR path)" >&2
    fail=1
  else
    echo "arch-ratchet ok: $key=$cur <= $base"
  fi
done < "$BASE_FILE"
exit $fail
