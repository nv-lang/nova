"""Read-only source witness, NOT a Nova execution. Run from a repo root or pass it."""
from pathlib import Path
import re
import sys

sys.stdout.reconfigure(encoding="utf-8")
root = Path(sys.argv[1] if len(sys.argv) > 1 else ".")

def source(rel):
    p = root / rel
    assert p.is_file(), f"missing source: {rel}"
    return p.read_text(encoding="utf-8")

def body(rel, signature):
    text = source(rel)
    start = text.index(signature)
    end = re.search(r"\n(?:export )?fn ", text[start + len(signature):])
    stop = start + len(signature) + end.start() if end else len(text)
    return text[start:stop]

def cite(rel, needle):
    text = source(rel)
    assert needle in text, f"lost witness: {rel}: {needle}"
    print(f"{rel}:{text[:text.index(needle)].count(chr(10)) + 1}: {needle}")

spec = "spec/decisions/07-modules.md"
registry = "novac/src/sem/coerce.nv"
ret = "novac/src/check/return_rules.nv"
collect = "novac/src/sem/collect.nv"
index = "novac/src/resolve/resolve.nv"
cite(spec, "callable row не участвует в duplicate/coherence judgments, shadowing,")
cite(index, 'if target_os != "" && !row_applies(fns.rows[i], target_os)')
cite(collect, "harvest_coerce_pair(fns, coerces, mrow)")
cite(registry, "coerces.push(CoercePair { from_id:")
cite(registry, "if p.from_id == from && p.to_id == to { return Some(p.via_row) }")
cite(ret, "ro ours = @ctx.fns.compiles_body(via) && fd.tparam_cnt == 0")
cite(ret, "@chan.record_coercion(e.id_of(), via)")
cite(ret, "@chan.record_wanted(via, []TyId.new())")
for rel, sig in [(registry, "export fn coerce_row("),
                 (registry, "export fn harvest_coerce_pair("),
                 (ret, "fn Checker mut @coerce_return(")]:
    code = "\n".join(line.split("//")[0] for line in body(rel, sig).splitlines())
    assert not re.search(r"target_state|target_os|selected_fns", code), sig

# A row is retained by D495. These are evaluations of the cited predicates,
# not a claim that a compiler fixture was run. from/to are arbitrary TyIds.
row = dict(target_mask=1, has_recv=True, param_cnt=1, ret_valid=True,
           own=True, tparam_cnt=0, source=11, dest=12, via=0)
linux_active = (row["target_mask"] // 2) % 2 == 1
registered = row["has_recv"] and row["param_cnt"] == 1 and row["ret_valid"]
pair_lookup = row["via"] if registered else None
ours = row["own"] and row["tparam_cnt"] == 0
recorded = pair_lookup is not None and ours  # admitted receiver mode assumed
assert not linux_active and registered and recorded
print("SOURCE_MODEL target=linux mask=windows canonical_index=false pair_registered=true coerce_return_records=true")
print("CONTROL active=windows canonical_index=true; no_pair => no conversion; receiver_mode assumed admitted")
print("НАХОДКА | К6 | sem | coerce-return | inactive own #coerce row bypasses candidate index through pair registry; return records via_row")
