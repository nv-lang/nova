# PROGRESS-p315 — fallible/effectful `@cleanup` enforcement (№315, D432 amendment 2026-08-04)

Модель: sonnet. Worktree `nova-p315`, ветка `p315-cleanup`. Design note written
BEFORE code per window rules.

## Bug recap

`consume X = e { body }` (block form, D188/D314) desugars into
`{ ro X = e; defer(o) { X.@cleanup(o) }; body }` AFTER all effect checks
(`check_module_impl`/`infer_effects` run first). The synthesized
`X.@cleanup(o)` call is invisible to effect checking, so a function using a
block-form consume of a fallible/effectful `@cleanup` type can silently gain
a hidden `Fail[E]`/effect without declaring it. D158 (`check_defer_body`)
only looks at explicit `throw`/`?`/`!!` in defer-body TEXT, not this
synthesized call. Same gap applies to bare `consume X = e;` left `Live` at
scope-exit — today ONLY effect-pure (`effects.is_empty()`) `@cleanup` types
are eligible for the affine auto-cleanup bypass (`LinearityRegistry::
cleanup_pure_types` / `ConsumeCtx::check_obligations_at_exit`); fallible
cleanup stays strictly linear, which is why `File`/`BufWriter[W]` were
excluded from auto-cleanup entirely (fs.nv:181, buffered.nv:26).

## Amendment (spec/decisions/02-types.md D432 §1, 2026-08-04) — what changes

1. Auto-inserted `@cleanup` call's effects are **DIRECT** effects of the
   enclosing function (not transitive) — D62 distinction matters (transitive
   effects only warn).
2. `Fail[E]` in cleanup's row must appear in the enclosing fn signature — NOT
   a new rule, `Fail` is already exception-to-transitivity (D65/D28); №315 is
   a checker defect (desugar runs after effect-checks), not a language gap.
3. Private fn: compiler infers/adds effects itself (D28/D62, unchanged
   policy). `export fn`: direct effects must be explicit — missing → compile
   error pointing at the BINDING that caused it (not just the callsite).
4. Cleanup failure on an already-failing path (`Failure`/`Panic` outcome)
   ATTACHES to the original cause, does not replace/swallow it (suppressed-
   exception model, mirrors existing `defer`+Fail pocket/`suppressed()`
   machinery, D158-D162).

Consequence named explicitly in the amendment: `File`/`TcpStream`-class
exclusion from auto-cleanup is lifted; containers are NOT affected (§2, no
drop-glue) — orthogonal, untouched by this window.

## Implementation plan (chosen after reading the actual code — narrower than
the pre-amendment 5-point consultant sketch quoted in the marker text,
because several of those points are now moot or out of scope for one window)

### Checker (types/mod.rs) — the actual №315 fix

- `LinearityRegistry`: `cleanup_pure_types: HashSet<String>` →
  `cleanup_effect_rows: HashMap<String, Vec<TypeRef>>` (type name → its
  `@cleanup`'s effect row, ANY row, not just empty). Also fixes a real
  pre-existing asymmetry: `build()` never walked `module.peer_files` while
  codegen's mirror (`emit_c.rs` `auto_cleanup_types` pre-pass) already does
  — folder-module co-files could silently diverge checker vs. codegen.
  `absorb_external` (builtin/implicit-reach path) intentionally still does
  NOT populate this map — pre-existing carve-out backed by
  `neg/permit_leak_neg.nv` and siblings (guard reached only via builtin path
  stays strict-linear); untouched.
- `has_pure_cleanup` kept (empty-row test derived from the map) +
  new `cleanup_effects(ty) -> Option<&Vec<TypeRef>>`.
- New free functions near `has_fail_effect`: `missing_cleanup_effect_names`
  (name-only match, same precedent as `has_fail_effect` — generic-arg
  mismatch on `Fail[E]` is not this check's concern) and
  `d432_cleanup_effect_diag` (builds `[E_D432_CLEANUP_EFFECT_NOT_DECLARED]`,
  message modeled on D158-defer-fail-not-in-sig's shape but general).
- **Block-form site** (`TypeCheckCtx::validate_consume_scope_init`, run from
  `check_consume_scopes_in_block/_stmt/_expr` during `check_module_impl`,
  i.e. BEFORE desugar — this is deliberately the SAME early pass that
  already validates `@cleanup`'s shape, D188-malformed-on-exit): threaded a
  new `current_fn_effects: Option<&[TypeRef]>` parameter through the whole
  walk (`Some(&fd.effects)` for `Item::Fn`, `None` for `Item::Test`/`Bench`
  — tests have no effect signature to declare against; same "ambient,
  unchecked" treatment the codebase already gives test bodies for effect
  declarations generally). After the existing `validate_on_exit_signature`
  call, when the row is non-empty and `current_fn_effects` is `Some`, run
  `missing_cleanup_effect_names` and push the new diagnostic pointing at the
  `consume` binding's init span if anything is missing.
- **Bare-form site** (`ConsumeCtx::check_obligations_at_exit`): added
  `current_fn_effects: Option<&'a [TypeRef]>` field (`Some(&f.effects)` for
  `Item::Fn`, `None` default for `Item::Test`). Where the leftover binding's
  type has ANY declared cleanup (not just pure), it stays eligible for the
  affine bypass; if the row is non-empty and effects are missing from
  `current_fn_effects` (when `Some`), push the new diagnostic instead of
  silently accepting (or of always emitting `D133-not-consumed`, which would
  be the wrong diagnosis — the type IS consumable, the problem is the
  missing effect declaration on ITS caller).
- `validate_on_exit_signature`'s old "effects must be `Fail[E]` only"
  structural gate (`D188-malformed-on-exit`, the *pre-amendment* §1 rule) is
  REMOVED — the amendment explicitly lifts that restriction; any effect is
  now legal on `@cleanup`, checked at each auto-cleanup SITE instead of at
  the declaration.
- Codegen mirror (`emit_c.rs` `auto_cleanup_types` pre-pass, ~line 5093-5141):
  same `f.effects.is_empty()` gate removed, kept in lockstep with the
  checker (the file's own comment states this MUST stay in sync or a
  mismatch silently leaks a resource / CC-FAILs).

### Known, deliberate scope narrowing (documented, not hidden)

- No "inside a local `with Eff = handler { ... }` block" escape hatch for
  the new diagnostic (D158's `inside_fail_handler_depth` has this for
  `Fail`; this window's check does not, for ANY effect). Only the enclosing
  function's OWN declared effect row is consulted. Chosen for scope/time —
  none of the brief's five required fixtures need it, and `Fs`/`Net` already
  have `#default_handler`s so existing std code declares the effect
  directly rather than wrapping locally. If `nova check std/src`/polaris
  canon gates shift because of this, that is the signal to revisit — they
  did not (see Gates below).
- Declaration-site validation of `@cleanup`'s signature shape (moving
  `validate_on_exit_signature` so a malformed cleanup is caught even if the
  type is ONLY ever reached via bare-form/generic-bound, never block-form)
  is NOT done — separate pre-existing gap from №315 itself, not required by
  the amendment text, left as a follow-up if the owner wants it flagged.

### Std (Part 2) — File/BufWriter[W] rollout

- `File` (`std/src/fs/fs.nv`): new `@cleanup(outcome ScopeOutcome) Fs
  Fail[IoError] -> ()` calling `@close()?` (propagates via the SAME
  Fail-propagation path as any other `?` — the "attach not replace" runtime
  behavior on an already-failing scope is the pre-existing defer+Fail
  suppressed-pocket machinery, D158-D162, not new code). Exclusion comment
  removed/replaced with a pointer to the D432 amendment.
- `BufWriter[W]` (`std/src/io/buffered.nv`): same treatment — `@cleanup`
  wraps `@close()`.
- Regression guards that assumed strict-linearity updated to match the new,
  intentionally-relaxed behavior (see report for the exact diff — this is
  the "what changed in std behavior" section).

## Verification order

1. `cargo build --release` (compiler-codegen) — must be clean.
2. Own 5 fixtures (`spec_tests/conformance/{,neg/}d315_*.nv`) — literal
   runner output pasted into the final report, not paraphrased.
3. `nova check std/src` — canon 148/26/61, explain any delta line-by-line.
4. `nova test std/src/fs` and `nova test std/src/net` — verdicts verbatim.
5. polaris `./nova.sh test src --strict-effects` — canon 37/0/18.
6. `scripts/guards/arch-ratchet.sh`.
7. `nova lint` on every touched `.nv` file.

Mega-CU / flagship — intégrator's job at acceptance, not this window's.
