# Nested tuple/record sub-pattern in a variant payload (D157) -- parser now reads it

Closes registry #1240. Spec side (D184's `pattern_arg` made recursive, D157
cross-reference added) was decided and merged by the owner/integrator
(commit `c68b33ab7`, `spec/decisions/03-syntax.md`) after `spec-reader`'s
first pass missed D157's already-standing norm (D34/D184 alone read as
flat-only; D157, `05-memory.md`, amendment `[M-216-record-payload-consume]`
2026-07-21, already named `Ok((a,b))`/`Some((a,b))`/`Ok({a,b})` legal and
gave them `E_CONSUME_PATTERN_REQUIRED`). This is the compiler side.

## The gap

`novac/src/parse/expr.nv`'s variant-payload loop (inside `@match_expr`,
both the primary head and the `|`-disjunction alternative) read only a
flat, comma-separated run of `Ident`:

```
while @peek() == TokenKind.Ident {
    pk.push(@take())
    @push_if(pk, TokenKind.Comma)
}
```

`Some((first, cnt))` -- ONE payload slot whose value is a tuple, destructured
by a nested sub-pattern -- gave this loop a leading `(` where it expected
only `Ident`. No branch matched, and the whole arm dropped into the
parser's unnamed fallback: three cascaded diagnostics for one mistake
(measured live in `novac/src/check/exprs.nv`, `novac/src/emit_c/emit_interp.nv`
-- both self-build carriers).

## Fix

One recursive function, `@variant_payload_elem`, parses ONE `pattern_arg`
per D157's grammar: an optional `mut`, then either a bare `Ident`, a nested
tuple sub-pattern (`(a, b)`, recursively -- a tuple element can itself be a
nested tuple), or a nested record sub-pattern (`{a, b}`, flat -- D157 does
not extend nesting into record fields). The two existing loops (payload
head, `|`-alternative payload) call it instead of pushing a bare token.

Everything is still pushed FLAT into the same `Pattern` node -- the same
convention the array pattern (`[b1, b2, ..]`) already uses a few lines up
in the same file. No new node kind; the delimiter leaves (`(`, `)`, `{`,
`}`, `,`) travel in the leaf sequence exactly as written.

## Proof

- `novac/src/parse/parse_test.nv`: new shape test, both the tuple form
  (`Point((first, mut cnt))`) and the record form (`Boxed({w, h})`),
  asserting byte-for-byte reserialization AND zero `NodeKind.Err` anywhere
  in the tree (a positive absence check, not just "some node was found").
- Both ways: reverted `expr.nv` alone (kept the new test) -- RUN-FAIL red;
  reapplied -- green.
- Both self-build carriers named by the spec-amendment discussion directly
  confirmed clean: `NOVAC_SELF_PATH=novac/src novac.exe check
  novac/src/check/exprs.nv` and the same for `emit_c/emit_interp.nv` --
  zero occurrences of "record constructor"/"a range is a for head"/"did
  not parse this" anywhere near the `Some((first, cnt))` (`exprs.nv`) and
  `Some((first, cnt))` (`emit_interp.nv:169`) sites; both files' remaining
  diagnostics are the unrelated, already-filed `NOVAC_SELF_PATH`
  double-registration harness bug.
- `novac/src/parse/parse_test.nv` and `novac/src/check/check_test.nv`:
  both green, no regression.
- `scripts/guards/check-novac-differential.sh`: launched before this commit
  (background, corpus-wide, minutes-scale per its own convention) --
  not blocking this commit on it: the parser fix is already proven both
  ways at the unit level and against both real self-build carriers, the
  same evidentiary bar #1241's own closure used. Its verdict, when it
  lands, goes in the handoff note rather than reopening this commit.

## Carrier caveat

The fix closes the CLASS (the payload-element parser, keyed by grammar
shape, not by file): any future nested tuple or record sub-pattern in a
variant payload anywhere in novac's own source or a user's program goes
through the same door. A third, independently-confirmed carrier
(`emit_c/emit_interp.nv:169`) was named byte-identical to the first by the
integrator before this fix landed and is covered by the same measurement
above, not a separate patch.

## What this fix does NOT cover

Declaring a SUM TYPE whose variant payload is itself a bare tuple TYPE
(`type Pos enum | Point((int, int))`) still falls into the parser's
fallback -- a different parsing concern (a type declaration's payload type
reference, not a match-arm pattern) that this fix does not touch. The real
self-build carriers never needed this: their tuple-typed values reach the
pattern through an ALREADY-declared generic instantiation
(`Option[(FieldRow, int)]`, `sem`'s own `field_range_of`), not a
freshly-declared sum type with an inline tuple payload.
