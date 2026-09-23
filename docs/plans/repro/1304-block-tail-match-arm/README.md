# A match arm's Block body silently dropped its tail value — novac printed 0 where the oracle prints 15

Found by window Carina, 2026-09-22, as the fourth position of the
`arm_pos` wave (registry #1296 named it protective then; the protection
was itself the bug). Closed the same day by commit `60c607b0b`.

## The defect

```nova
fn f(k int) -> int {
    match k {
        0 => { ro x = 10
               x + 5 }
        _ => 3
    }
}
fn main() { println(f(0)) }
```

The oracle prints **15**. novac printed **0** — and neither side said a
word. Not a subset refusal: a silent miscompile, the worst kind of green.

## Why it stayed invisible

The cause was an AGREEMENT between the two halves, so no in-tree guard
could see a disagreement:

- `check/match_arms.nv`'s `@type_arm` (wave B9) answered `None` for EVERY
  Block-bodied arm **by explicit design** — "the arm itself yields no
  value to the fold". The fold then let the match's type come from
  whichever OTHER arms produce one.
- `lower_match.nv`'s `@lower_arm_body` mirrored it exactly: every Block
  arm counted as `leaves` unconditionally and went to statement mode.

Checker and lowering agreed with each other; what neither agreed with
was the LANGUAGE, where a block's bare tail expression is its value the
same way a function body's is. The pre-declared result temporary of
`@lower_match` was simply never written — the zero came from there.

**Lesson (recorded in the Carina handoff):** the checker and the lowering
can err CONSISTENTLY, and then no guard between them fires — the
discrepancy was searched BETWEEN the halves while both had moved against
the language together. The only witness is the BEHAVIORAL diff against
the oracle (`novac-e1-smoke.sh`), never the verdict of `check`.

## Fix — three layers, each found by testing, not by reading ahead

1. `check.nv` — the shape walk carries the arm's value position down to
   the block's TAIL child (`tail_at`, computed once beside `body_at` and
   `of_lparen_at`, only for a non-terminating Block that really is an arm
   body). Without it, kinds that GATE on a position (a record ctor, an
   array literal, an `if` value) were still refused while typing and
   lowering already carried their value.
2. `check/match_arms.nv` — `@type_arm`'s Block case asks
   `block_terminates` first (a block that leaves on every path keeps its
   old answer — the tail is dead code), and otherwise types the tail via a
   new `@type_arm_block_value`. That door mirrors `@type_branch_value` but
   with OPTIONAL value semantics: a non-expression tail falls through to
   `@type_stmt` instead of being refused, because `@type_match` folds
   every arm unconditionally and a statement-only Block arm must keep
   compiling exactly as before. The type is RECORDED on the block node —
   `lower` has no edge to `check` in the architecture table and reads the
   answer from the channel.
3. `lower/lowering.nv` + `lower/lower_match.nv` — a new
   `@lower_block_stmts_value(b, dest)`: the `tail=true` path of
   `@lower_block_stmts_fn` generalized from `@ir.ret_place()` to an
   arbitrary destination. `@lower_arm_body` routes a Block through it when
   the channel says the block carries a type. A NEW function rather than a
   parameter on the shared one: `@lower_block_stmts_fn` serves every
   function body's own tail, and widening its signature would owe a
   separate proof on that unrelated path.

## Proof (from commit `60c607b0b`, both ways on the real rebuilt binary)

- **RED** (all four files reverted, novac rebuilt from the unpatched
  source): the probe smokes `< 15 / > 0` — the miscompile reproduced on
  untouched code.
- **GREEN** (patch restored, rebuilt): `novac-e1-smoke.sh` is byte-for-byte
  identical to the oracle, exit 0.
- A record-constructor tail (the original carrier shape from
  `emit_c/emit_interp.nv`: `PfFloat => { if ... { return ... } DisplayCall{...} }`)
  smokes byte-identical too — that is what layer 1 bought.
- `check_test` / `lower_test` green; the new checker test pins all three
  answers — agreeing arms, DISAGREEING arms (the control proving the
  tail's type really reaches the fold), and a returning Block keeping its
  old silence.

## Known-open risk (named in the handoff, not measured)

The fix CHANGES behavior: a Block arm now participates in the type fold,
so code that used to compile because the Block arm stayed silent may now
honestly redden with "arms disagree". The differential has not been run
after this fix at the time of the handoff. Running it is the next step of
the same wave, not a separate decision.
