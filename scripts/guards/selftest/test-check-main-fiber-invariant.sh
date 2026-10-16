#!/usr/bin/env bash
# Self-test for check-main-fiber-invariant.sh.
set -u
export LC_ALL=C

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
G="$ROOT/scripts/guards/check-main-fiber-invariant.sh"
FAILED=0
ok() { echo "  ok: $1"; }
bad() { echo "  ПРОВАЛ: $1" >&2; FAILED=1; }
T="$(mktemp -d)"
trap 'rm -rf "$T"' EXIT

mkdir -p "$T/scripts/guards" "$T/spec/decisions" "$T/compiler-codegen/nova_rt"
cp "$G" "$T/scripts/guards/"
cat > "$T/spec/decisions/04-effects.md" <<'EOF'
## D493. Маркерные эффекты безопасности в строках
D446 (safety функциональных значений)
EOF
cat > "$T/spec/decisions/06-concurrency.md" <<'EOF'
Context before invariant.

**Инвариант главного файбера (текущее устройство рантайма).**
D493 [279](../../docs/plans/279-mn-safety-via-effects.md); the root main fiber does not migrate.

Context after invariant.
EOF
cat > "$T/compiler-codegen/nova_rt/runtime.c" <<'EOF'
[INV-GUARD: check-main-fiber-invariant.sh] The root main fiber must
EOF
cat > "$T/compiler-codegen/nova_rt/fibers.h" <<'EOF'
[INV-GUARD: check-main-fiber-invariant.sh] The root main fiber is resumed
EOF

if bash "$T/scripts/guards/check-main-fiber-invariant.sh" "$T" >/dev/null; then
    ok "принимает связанный D493/D446 и обе runtime-двери"
else
    bad "ложно краснит на полном наборе invariant links"
fi

# Exact acceptance probe: remove only the invariant paragraph, preserving the
# surrounding document; RED must name the file, restoration must GREEN.
cp "$T/spec/decisions/06-concurrency.md" "$T/spec/decisions/06-concurrency.saved"
awk 'BEGIN { skip = 0 }
     /^\*\*Инвариант главного файбера/ { skip = 1; next }
     skip && /^$/ { skip = 0; next }
     !skip { print }' \
    "$T/spec/decisions/06-concurrency.md" > "$T/spec/decisions/06-concurrency.md.tmp"
mv "$T/spec/decisions/06-concurrency.md.tmp" "$T/spec/decisions/06-concurrency.md"
OUT=$(bash "$T/scripts/guards/check-main-fiber-invariant.sh" "$T" 2>&1); RC=$?
if [ "$RC" -ne 0 ] && echo "$OUT" | grep -q 'spec/decisions/06-concurrency.md' \
   && grep -q 'Context before invariant.' "$T/spec/decisions/06-concurrency.md" \
   && grep -q 'Context after invariant.' "$T/spec/decisions/06-concurrency.md"; then
    ok "удаление только абзаца инварианта RED с именем файла; соседний текст сохранён"
else
    bad "удаление абзаца инварианта не стало RED/соседний текст потерян (код $RC): $OUT"
fi
mv "$T/spec/decisions/06-concurrency.saved" "$T/spec/decisions/06-concurrency.md"
if bash "$T/scripts/guards/check-main-fiber-invariant.sh" "$T" >/dev/null; then
    ok "восстановление строки инварианта GREEN"
else
    bad "после восстановления invariant line guard всё ещё красный"
fi

if [ "$FAILED" -eq 0 ]; then
    echo "селфтест check-main-fiber-invariant: 3/3 ok"
    exit 0
fi
echo "селфтест check-main-fiber-invariant: ЕСТЬ ПРОВАЛЫ" >&2
exit 1
