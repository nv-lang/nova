#!/bin/sh
# Самотест стража scripts/guards/check-marker-registry-sync.sh (план 231 трек Ж).
#
# Доказывает ОБА свойства, как требует конвенция:
#   (1) ЛОВИТ нарушение   — маркер в .nv без записи в реестрах поднимает счётчик выше
#                            baseline, страж краснеет;
#   (2) НЕ ДАЁТ ЛОЖНЯКА   — тот же маркер, занесённый в реестр, снова зелёный.
#
# Работает на ВРЕМЕННОМ дереве в $TMPDIR — настоящую репу не трогает.
set -u
GUARD="$(cd "$(dirname "$0")/.." && pwd)/check-marker-registry-sync.sh"
[ -f "$GUARD" ] || { echo "SELFTEST FAIL: страж не найден: $GUARD" >&2; exit 1; }

TMP="${TMPDIR:-/tmp}/mrs_selftest_$$"
rm -rf "$TMP"; mkdir -p "$TMP/std/src" "$TMP/examples" "$TMP/spec_tests" "$TMP/docs/plans" "$TMP/scripts/guards"

# Реестры (пустые, но существующие — как в настоящей репе).
: > "$TMP/docs/plans/221.1-bug-sweep.md"
: > "$TMP/docs/plans/backlog-followups.md"
: > "$TMP/docs/dev/simplifications.md"
echo "unregistered=0" > "$TMP/scripts/guards/marker-registry.baseline"

# --- (1) НАРУШЕНИЕ: маркер в коде, в реестрах его нет ---
cat > "$TMP/std/src/probe.nv" <<'EOF'
// [M-selftest-orphan-marker] обход: причина не заведена ни в один реестр
fn probe() -> int => 1
EOF

if sh "$GUARD" "$TMP" >/dev/null 2>&1; then
    echo "SELFTEST FAIL: страж НЕ поймал неучтённый маркер (свойство 1 не выполнено)" >&2
    rm -rf "$TMP"; exit 1
fi

# --- (2) НЕТ ЛОЖНЯКА: тот же маркер занесён в реестр ---
echo "| 999 | \`[M-selftest-orphan-marker]\` — тестовая запись | P3 |" > "$TMP/docs/plans/221.1-bug-sweep.md"

if sh "$GUARD" "$TMP" >/dev/null 2>&1; then
    :
else
    echo "SELFTEST FAIL: страж краснеет на УЧТЁННОМ маркере (ложняк, свойство 2 не выполнено)" >&2
    rm -rf "$TMP"; exit 1
fi

# --- (3) ХРАПОВИК: долг в пределах baseline не краснеет ---
rm -f "$TMP/docs/plans/221.1-bug-sweep.md"; : > "$TMP/docs/plans/221.1-bug-sweep.md"
echo "unregistered=1" > "$TMP/scripts/guards/marker-registry.baseline"
if sh "$GUARD" "$TMP" >/dev/null 2>&1; then
    :
else
    echo "SELFTEST FAIL: храповик не пропускает долг в пределах baseline" >&2
    rm -rf "$TMP"; exit 1
fi

rm -rf "$TMP"
echo "selftest check-marker-registry-sync: OK (ловит нарушение / без ложняка / храповик работает)"
exit 0
