# §10a rename progress — `unsafe` type-modifier → `uninit`

Worktree: `nova-nt`, branch `uninit-rename-d216`. Model: **sonnet**.
Status: **DONE** (2026-07-11), single pass, no rate-limit interruption.

## Scope decision (load-bearing — read before touching this again)

`unsafe` was overloaded: (1) type-modifier `*unsafe T` / `unsafe T` (possibly-uninit
pointee/value) and (2) `unsafe { }` block + `unsafe fn`/`external unsafe fn` attribute.
Renamed ONLY (1) → `uninit`. (2) untouched.

**Subtlety found during implementation (not in the original task brief):** the
fn-pointer-type composition `*unsafe fn(...)` / `*extern "C" unsafe fn(...)` (D216 §10)
is ALSO structurally `Pointer(Unsafe(Func))` — i.e. the exact same AST wrapper as the
data-uninit case, just with `Func` as payload. Confirmed via `docs/plans/174.5-pointer-ops-methods.md`
заход 2026-07-09 note: "Rename `unsafe T`→`uninit T` (data-uninit; `unsafe{}`/`unsafe fn`/
`*unsafe fn` не трогаем)" — so `*unsafe fn(...)` is DELIBERATELY excluded from the rename
(it encodes call-requires-unsafe, not possibly-uninit data). The parser disambiguates by
payload: `unsafe` in type position is legal ONLY when the wrapped type is `Func`; any other
payload is a hard error pointing at `uninit`. Both spellings construct the SAME AST node
(`TypeRef::Uninit`, internal name) — there is no separate variant for the fn-pointer shape.

## What changed (surface)

- New keyword `uninit` (`TokenKind::KwUninit`, `lexer/mod.rs` + `lexer/token.rs`).
- `*uninit T` (pointer to possibly-uninit T, postfix pointee) / `uninit T` (value-wrapper).
- `unsafe` in type position wrapping non-`Func` → hard error `E_UNSAFE_TYPE_MODIFIER_RENAMED`
  (parser/mod.rs, mirrors the existing `E_KW_REMOVED_READONLY`/`E_SAFE_RETIRED` pattern).
- `unsafe` in type position wrapping `Func` (`*unsafe fn(...)`, `*extern "C" unsafe fn(...)`,
  bare `unsafe fn(...)`) — UNCHANGED, still legal, still spelled `unsafe`.
- `unsafe { }` blocks, `unsafe fn` / `external unsafe fn` declarations — UNCHANGED.

## What changed (internals)

- AST: `TypeRef::Unsafe(Box<TypeRef>, Span)` → `TypeRef::Uninit(...)` (ast/mod.rs). Renamed
  UNIFORMLY (mechanical) — used both for the renamed data-uninit case AND the unrenamed
  fn-pointer-type case (same node, disambiguated only at parse time by payload).
  `TypeRef::is_unsafe()` → `TypeRef::is_uninit()` (zero callers, safe rename).
- `PointerModifier::Unsafe` → `PointerModifier::Uninit` (ast/mod.rs, Ty-level tag used by
  `types/mod.rs` `ResolvedType::TypedPtr`).
- Parser (`parser/mod.rs`, `parse_type`): new `KwUninit` arm (generic, any T, same
  right-binding/prefix-forbid template as before) + narrowed `KwUnsafe` arm (prefix-forbid
  check unchanged, then hard-errors unless inner is `Func`) + `unsafe_type_modifier_renamed_error`
  helper (new, alongside `pointer_prefix_modifier_error`/`redundant_pointer_ro_error`).
  `*extern "C" unsafe fn(...)` composition arm — untouched except AST-variant rename.
- Display/diagnostic rendering — Func-conditional (`unsafe` if payload is `Func`, else
  `uninit`) in: `types/mod.rs` (`typeref_display` ×2, `render_type_ref`, `typeref_render`),
  `codegen/emit_c.rs` (`type_ref_overload_key`), `doc/collector.rs` (`render_type`),
  `nova-lsp/src/symbol.rs` (`format_type_ref`). `doc/render_json.rs` structural-type `kind`
  label renamed uniformly `unsafe_wrap` → `uninit_wrap` (internal JSON schema tag, not surface
  syntax — `source` field already carries the correct keyword).
- All other `TypeRef::Unsafe`/`PointerModifier::Unsafe` pattern matches (transparent-wrapper
  recursion in lints.rs, external_registry.rs, share_check.rs, field_cache.rs, gc_layout.rs,
  may_gc.rs, overload_sig.rs, const_fn_eval.rs, nova-lsp/{completion,type_definition,
  semantic_tokens,symbol}.rs, compiler-codegen/tests/plan172_14_blast_radius.rs) — mechanical
  rename, no behavior change (structurally identical, name only).
- `nova-lsp/src/semantic_tokens.rs` `is_keyword` — added `KwUninit`. `completion.rs` keyword
  list — added `("uninit", ...)` entry alongside `("unsafe", ...)`.
- Editors (D278 keyword-highlight governance): `editors/vscode/syntaxes/nova.tmLanguage.json`,
  `editors/vim/syntax/nova.vim` — added `uninit` to the modifier keyword group.
  `editors/zed/languages/nova/highlights.scm` — GAP comment updated (tree-sitter grammar
  doesn't have either `unsafe` or `uninit` yet, pre-existing gap, now notes both).
  `compiler-codegen/tests/syntax_highlight_conformance.rs` `ACTIVE` list — added `"uninit"`.
  **NOT touched:** `www` repo's `check-highlight-keywords.mjs` guard — out of scope (task
  restricted work to the `nova-nt` worktree only; www is a separate repo).

## .nv migration

**Zero files needed migration.** Grepped `std/` and `spec_tests/` before starting: no
`*unsafe T` / bare `unsafe T` (data) usage anywhere — only `unsafe fn`/`external unsafe fn`
declarations and `unsafe { }` blocks (all untouched). `nova_tests/` (not a correctness gate)
has old `*unsafe T` fixtures under `plan118/`, `plan118_5/` — left as-is, out of scope.
`examples/typed_pointers/basic_pointer.nv` also has `*unsafe T` usage — left as-is (examples
not gated; could be swept in a follow-up if desired).

## Tests added

- `spec_tests/conformance/neg/d216_unsafe_type_modifier_renamed_neg.nv` — bare `unsafe T`
  value-wrapper form → `E_UNSAFE_TYPE_MODIFIER_RENAMED`.
- `spec_tests/conformance/neg/d216_unsafe_ptr_modifier_renamed_neg.nv` — `*unsafe T` pointer
  form → same error.
- `spec_tests/conformance/d216_uninit_rename_174_5.nv` (positive, shared CU) — `uninit T` /
  `*uninit T` param types compile + read through pointer; legacy `*unsafe fn(...)` local-var
  bind + call still works unchanged.

## Spec amendment

`spec/decisions/02-types.md`, D216 territory:
- Anchor lines updated to current `*uninit T` syntax: the `-> *unsafe T` return-type table
  (~3122), the `*T`/`*mut T`/`*unsafe T` prose definition (~7554-7556, ~8149-8154), the
  prefix-forbid example block (~8191-8208), the §11a pointer-methods table, the §12 casts
  table, the `## D216 V2 amend` canonical Token table (`unsafe T`/`*unsafe T`/`unsafe * T`
  rows), the FINAL-chains example block + FFI out-param canonical example (§V2.2).
- New subsection **`### §10a rename — \`unsafe\` type-modifier → \`uninit\` (Plan 174.5,
  2026-07-11)`** appended at the end of the `## D216 V3 amend` chain (after §V3.7, before
  the unrelated `### D52 amend` sub-topic that follows in this same grab-bag heading) —
  full rationale, what changed / what didn't, the fn-pointer-composition subtlety, hard-error
  code, tests, and implementation-site summary. This is the authoritative reference; older
  prose deeper in the V2/V3 historical narrative (§V2.3, §V3.2, D218 cross-refs, ~60 more
  `*unsafe T` mentions) was deliberately NOT bulk-rewritten — read those historical passages
  as `uninit T`/`*uninit T` wherever they describe the DATA-uninit wrapper (not `unsafe {}`/
  `unsafe fn`/`*unsafe fn(...)`), per the new §10a banner. Rationale: this file is in the
  actively-edited "zone 172" (per nova-private note referenced in the task brief) — a full
  mechanical sweep of ~90 scattered historical/prose occurrences (mixed with genuine prose
  uses of the word "unsafe" referring to the block/operation concept, which must NOT be
  renamed) was judged higher-risk than a clearly-marked, comprehensive amendment banner at
  the canonical reference points + tables. No error-index.md exists in this repo — per
  `compiler-conventions.md` §5/§9 the D-block itself IS the distributed error-index entry.

## Gates (tally)

- (а) `cargo build --release` (nova-cli manifest) — clean, 0 errors (only pre-existing warnings).
- (б) `nova test --positive --compile-error spec_tests/conformance` — **93/0** (was 91/0
  baseline; +2 = the two new neg tests; the new positive file folds into the existing
  single-CU positive run with no separate PASS line, confirmed via standalone run: 1/0,
  both `test` blocks pass).
- (в) `nova check std/` — 25 FAIL, **identical to pre-existing baseline** (verified: all 25
  are either intentional `*_neg.nv` fixtures that `nova check` naturally fails outside the
  `nova test` EXPECT_COMPILE_ERROR harness, or a pre-existing `E_D78_MODULE_PATH_MISMATCH` in
  `std/ffi/cstr.nv` and missing test helpers in `std/tls/cert_modes_test.nv` — grepped the
  full FAIL output for "unsafe"/"uninit": zero mentions, confirming no rename-caused
  regression).
- (г) grep-invariant: `*unsafe ` type-modifier count in `std/*.nv` = 0 (was already 0 before
  this change — nothing to migrate); `unsafe {` blocks and `unsafe fn`/`external unsafe fn`
  declarations intact (grepped, all present, e.g. `std/runtime/raw_mem.nv`,
  `std/ffi/cstr.nv`).
- (д) neg-test for `E_UNSAFE_TYPE_MODIFIER_RENAMED` — added (2 files, see above), both PASS.

## Known pre-existing gap found (NOT fixed — out of scope for this surgical rename)

Calling through a `*unsafe fn(...)`-typed **function PARAMETER** (as opposed to a local
`let`-bound variable, which works fine and is what `nova_tests/plan118_1_7` tests) produces
broken C (`void** f` param type + undeclared-identifier codegen for the call). Isolated and
confirmed this is unrelated to the rename: (1) the local-variable form still works correctly
post-rename (verified with the exact `nova_tests` scenario inline); (2) `std/` has zero usage
of `*unsafe fn(...)` as a parameter type; (3) no existing test ever covered the parameter
case; (4) the rename only changed enum-variant/keyword IDENTIFIERS, never the C-type-emission
branch logic (`resolved_type_to_c`'s `TypedPtr` arm double-wraps `R::Func`'s already-opaque
`"void*"` into `"void**"`). Flagging for a future `[M-...]` marker if someone wants to fix it;
not touched here (would be scope creep for a "rename the keyword" task).
