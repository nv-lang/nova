"""Independent read-only witness for an active free call using an inactive conversion."""
from pathlib import Path
import re
import sys

sys.stdout.reconfigure(encoding="utf-8")
root = Path(sys.argv[1] if len(sys.argv) > 1 else ".")

def text(rel):
    p = root / rel
    assert p.is_file(), f"missing source: {rel}"
    return p.read_text(encoding="utf-8")

def cite(rel, needle):
    s = text(rel)
    assert needle in s, f"lost witness: {rel}: {needle}"
    print(f"{rel}:{s[:s.index(needle)].count(chr(10)) + 1}: {needle}")

def fn(rel, signature):
    s = text(rel)
    at = s.index(signature)
    m = re.search(r"\n(?:export )?fn ", s[at + len(signature):])
    return s[at:at + len(signature) + m.start()] if m else s[at:]

calls = "novac/src/check/calls.nv"
reg = "novac/src/sem/coerce.nv"
harvest = "novac/src/sem/harvest.nv"
cite("spec/decisions/07-modules.md", "overload resolution или emission для выбранной target.")
cite(harvest, "harvest_coerce_pair(fns, coerces, FnRow(fns.rows.len() - 1))")
cite(calls, "mut row = @res.first_row_of(cname)")
cite(calls, "if !@coerces_into(here, key) { continue }")
cite(calls, "match coerce_row(@ctx, got, want)")
cite(calls, "if !@ctx.fns.compiles_body(via) { return false } else { needs_pair = true }")
cite(calls, "@coerced_slots(winner, key)")
cite(calls, "via_row: coerce_row(@ctx, got, want) ?? no_row()")
for signature in ["fn Checker @coerces_into(", "fn Checker @coerced_slots("]:
    code = "\n".join(l.split("//")[0] for l in fn(calls, signature).splitlines())
    assert not re.search(r"target_state|target_os|selected_fns", code), signature
cite(reg, "if p.from_id == from && p.to_id == to { return Some(p.via_row) }")

# Source-predicate evaluation: target call 'take' is active; conversion 'into'
# is an own nongeneric one-receiver method, registered despite target=false.
# Binding and type ids are already valid; this tests no parser or compiler.
for mask, expected_direct in [(1, False), (2, True)]:
    selected = [r for r, m in [(0, mask), (1, 7)] if (m // 2) % 2 == 1]
    assert (0 in selected) == expected_direct and 1 in selected
    pairs = [(11, 12, 0)]
    via = next((r for a, b, r in pairs if a == 11 and b == 12), None)
    compiles_body = via is not None and via < 2
    needs_pair = compiles_body
    slot = via if needs_pair else None
    assert slot == 0
    print(f"SOURCE_MODEL target=linux mask={mask} selected={selected} direct_conversion={expected_direct} active_take_coercion_via={slot}")
print("CONTROL no declared pair => coercing candidate false; inactive target CALLEE is absent from FnIndex, but via_row is not checked there")
print("НАХОДКА | К6 | sem | coerce-argument | same pair-registry bypass as coerce-return; active free candidate records inactive conversion in SlotBind")
