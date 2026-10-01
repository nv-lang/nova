#!/usr/bin/env bash
# p3 -- K4 inside the registry family: on ONE registry file the two readers of
# "is row N closed" answer OPPOSITELY. check-push-proven-by-ci.py claims in its
# header (lines 32-34): "CLOSED ROW -- the same reading as registry-routes-scan.py
# (the canon of the registry guards)". The two codes differ:
#   push-proven  : first literal `**Статус:**` (bold), 60 chars right after it;
#   routes-scan  : first `Статус:` (bold or not), skip spaces, 60 chars.
# Row 9005: real status plain "Статус: ЗАКРЫТ", prose later quotes the bold form.
# Row 9006: prose first mentions "`Статус:`", the real bold field says ЗАКРЫТ.
# Both rows carry 🔴 so routes-scan lists them in no_route_list iff it reads OPEN.
#
# A/B: push-proven with an exemption citing 9005 / 9006.
# C  : registry-routes-scan.py on the same file (open K1 rows are listed).
# D  : read-only measure of the REAL registry with both readings
#      (measure_real_registry.py reproduces the two functions verbatim).
# Run from the repository root: bash <this-dir>/cmd.sh
export LC_ALL=C
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="${NOVA_REPO_ROOT:-$PWD}"
G="$ROOT/scripts/guards/check-push-proven-by-ci.py"
RS="$ROOT/scripts/guards/registry-routes-scan.py"
PY=python3; "$PY" -c 'print(1)' >/dev/null 2>&1 || PY=python
SHA=1111111111111111111111111111111111111111
REG="$HERE/tree/docs/plans/221.1-bug-sweep.md"
for f in "$G" "$RS" "$REG" "$HERE/runs-red.json" "$HERE/jobs-red.json" "$HERE/acc-9005.list" "$HERE/acc-9006.list"; do
    [ -s "$f" ] || { echo "MISSING $f"; exit 2; }
done
for n in 9005 9006; do
    echo "=== push-proven, exemption cites #$n"
    "$PY" "$G" "$SHA" --runs-json "$HERE/runs-red.json" --jobs-json "$HERE/jobs-red.json" \
        --registry "$REG" --accepted "$HERE/acc-$n.list" 2>&1; echo "rc=$?"
done
echo "=== registry-routes-scan on the same file (no_route_list = rows it reads OPEN)"
NOVA_NOFIELD_BASELINE="$HERE/nofield-empty.baseline" "$PY" "$RS" "$HERE/tree" 2>&1; echo "rc=$?"
echo "=== D. real registry, both readings side by side (read-only)"
"$PY" "$HERE/measure_real_registry.py" "$ROOT/docs/plans/221.1-bug-sweep.md" | grep -v "push-proven=nofield"
echo "rows where push-proven says nofield (a refusal if cited) but routes-scan reads a status:"
"$PY" "$HERE/measure_real_registry.py" "$ROOT/docs/plans/221.1-bug-sweep.md" | grep -c "push-proven=nofield"
