"""Read-only inventory of alternative producers and consumers. No compiler claims."""
from pathlib import Path
import re
import sys

sys.stdout.reconfigure(encoding="utf-8")
root = Path(sys.argv[1] if len(sys.argv) > 1 else ".")

def read(rel):
    p = root / rel
    assert p.is_file(), f"missing source: {rel}"
    return p.read_text(encoding="utf-8")

def cite(rel, needle):
    s = read(rel)
    assert needle in s, f"lost witness: {rel}: {needle}"
    print(f"{rel}:{s[:s.index(needle)].count(chr(10)) + 1}: {needle}")

def code_fn(rel, signature):
    s = read(rel)
    start = s.index(signature)
    end = re.search(r"\n(?:export )?fn ", s[start + len(signature):])
    stop = start + len(signature) + end.start() if end else len(s)
    return "\n".join(l.split("//")[0] for l in s[start:stop].splitlines())

cfg = "novac/src/pipeline/cfg.nv"
collect = "novac/src/sem/collect.nv"
harvest = "novac/src/sem/harvest.nv"
rows = "novac/src/sem/callables.nv"
resolve = "novac/src/resolve/resolve.nv"
numeric = "novac/src/check/numeric_positions.nv"
handed = "novac/src/sem/handed_bodies.nv"
cite(cfg, "if is_function_item(child)")
cite(cfg, "out.append(pending)")
cite(cfg, "if kind == NodeKind.FnDecl { return true }")
cite(cfg, "if kind != NodeKind.ExportDecl { return false }")
cite(collect, "ro (target_mask, target_unknown_mask) = cfg_attr_outcomes(pending_cfg_attrs)")
cite(collect, "target_unknown_mask: target_unknown_mask)")
cite(harvest, "inherited_mask, inherited_unknown, target_os)")
cite(harvest, "target_mask: target_mask, target_unknown_mask: target_unknown_mask)")
cite(rows, "target_mask: fd.target_mask, target_unknown_mask: fd.target_unknown_mask")
cite(resolve, "fd.target_state(target_os) == 1")
print("FORMS free/method/static/generic/handed/export: common row mask or explicit preservation found; no additional bypass claimed on ordinary index paths")
cite(collect, "defs.add(leaf_text(branch_children(c)[1]), DefTarget.DefType(spec.under_decl))")
print("ALIASES: type alias resolves to underlying declaration; no separate callable-mask claim or executed alias fixture")
cite(handed, "Some(d) => !has_body(d)")
print("EXTERN: handed body absence has an explicit predicate; ordinary row indexing is shared, native extern behavior not tested")
cite(resolve, "@method_mode_for(t, ctx, sig, true).is_some()")
cite(resolve, "mut r = raw_row(@first_row_of(sig.name))")
print("READONLY protocol satisfaction: uses the target-filtered FnIndex, not a fresh all-row scan; no new carrier found here")

# Third semantic position: the checker accepts a declared pair even when
# the pair row is target-inactive. This is the SAME root as the other probes.
cite(numeric, "match coerce_row(@ctx, have, want)")
cite(numeric, "Some(_) => {")
judge = code_fn(numeric, "fn Checker mut @judge_value_in(")
assert not re.search(r"target_state|target_os|selected_fns", judge)
assert "@reject_first_leaf_of" in judge and "Some(_) => {" in judge
print("SOURCE_MODEL typed field/payload with have!=want, nonliteral/nonparam/nonnever: pair=Some(inactive_row) suppresses mismatch here; no_pair follows rejection branch")
print("НАХОДКА | К6 | sem | forms-consumers | same inactive coerce-pair root affects typed field/payload mismatch judgment; not a new claim about existing missing coercion emission")

# Read-only ABI consumers are recorded only as leads: preservation of ABI
# overload ordinals may have independent compatibility constraints.
for signature in ["fn Ctx @overload_count(", "fn Ctx @free_overload_ordinal(", "fn Ctx @overload_ordinal("]:
    code = code_fn(handed, signature)
    assert not re.search(r"target_state|target_os|selected_fns", code)
    cite(handed, signature)
print("LEAD_ONLY readonly overload counters count retained declarations without target test; ABI correctness and target shell symbols were not measured, so no finding assigned")

tests = read("novac/src/pipeline/cfg_fn_test.nv")
assert "selects one target row" in tests and "methods static calls and export wrappers" in tests
assert "#coerce" not in tests
print("GUARD_FORM cfg_fn_test: direct row/call/selected-body assertions present; no #coerce input in this file (not a claim about all tests)")
cite("scripts/guards/check-novac-row-fields.py", "src.add((rec, head_field(line)))")
cite("scripts/guards/check-novac-arch-class-proofs.py", 'if "**Верность:**" in line:')
print("GUARD_GAP declared fields and prose proof markers do not check applicability of via_row returned by pair lookup")
