<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# Plan 196 Ф.1b — refresh-audit (read-only, 2026-07-11)

Exact re-audit of the umbrella's rough recon numbers (`docs/plans/196-one-truth-closeout.md`
"Recon-факты" §3-4). Scope: `compiler-codegen/src/codegen/emit_c.rs` only (today's sole
offender file per whole-crate scan, `compiler-codegen/tests/no_raw_type_decode.rs`).

## 1. Raw `Nova_`/`____`-decode census (grep-invariant "0 outside `debt_*`")

Methodology: scan every non-comment line in `compiler-codegen/src/**/*.rs` for 7 literal
decode-needles (`.strip_prefix("Nova_")`, `.trim_start_matches("Nova_")`, `.contains("____")`,
`.find("____")`, `.split("____")`, `.split_once("____")`, `.rsplit_once("____")`), attribute
each hit to its innermost enclosing named `fn` (last-seen header line, any indent — covers
`impl` methods and nested local fns), then bucket by whether that function's name starts with
`debt_` (the sanctioned, tracked decode surface). Implemented as `compiler-codegen/tests/
no_raw_type_decode.rs`; independently re-derived here with an equivalent standalone `awk`
pass over `emit_c.rs` for cross-check — **both methods agree exactly**.

**Exact numbers (supersede the recon's rough "~70 хитов, 12 вне debt"):**

| | count |
|---|---|
| total raw-decode call sites (whole file) | **78** |
| inside `debt_*` helpers (sanctioned) | **56** |
| **outside `debt_*` (un-audited "second window")** | **22**, across **16 functions** |

The recon undercounted the outside-`debt_*` figure (12→22) and the function count (10→16);
the true total (78, not ~70) was likewise higher. Six functions the rough estimate missed:
`emit_for`, `collect_pattern_inner_bindings`, `register_container_eq_mono`,
`infer_mono_method_ret_with_args`, `fn_field_call_sig`, `infer_expr_c_type` itself (2 sites,
distinct from — and in addition to — the ~44 Channel-6z sub-arms catalogued in §2 below,
which are tracked separately under `[M-172.1-lifted-legacy-arms]`).

### The 22 sites (file:line, all `compiler-codegen/src/codegen/emit_c.rs`)

| # | file:line | function | snippet |
|---|---|---|---|
| 1 | emit_c.rs:8277 | `emit_protocol_box_typedef` | `if n.starts_with("Nova_") && n.contains("____")` |
| 2 | emit_c.rs:8298 | `emit_protocol_box_typedef` | `if (payload.contains("____") \|\| payload.ends_with("_p")` |
| 3 | emit_c.rs:14325 | `emit_value_record_type` | `if pointee.starts_with("Nova") && pointee.contains("____") {` |
| 4 | emit_c.rs:14393 | `emit_record_type` | `base.trim_start_matches("Nova_"),` |
| 5 | emit_c.rs:14572 | `emit_sum_type` | `base.trim_start_matches("Nova_"),` |
| 6 | emit_c.rs:20910 | `emit_generic_type_instance` | `base.trim_start_matches("Nova_"),` |
| 7 | emit_c.rs:25827 | `emit_expr_with_target_type` | `let ctor_base: &str = if sum.contains("____") {` |
| 8 | emit_c.rs:26986 | `emit_expr` | `let dispatch_type_name = if type_name_sum.contains("____") {` |
| 9 | emit_c.rs:27072 | `emit_expr` | `if let Some(idx) = recv_short.find("____") {` |
| 10 | emit_c.rs:27097 | `emit_expr` | `if let Some(idx) = type_name_sum.find("____") {` |
| 11 | emit_c.rs:31031 | `emit_call` | `if let Some(idx) = recv_type.find("____") {` |
| 12 | emit_c.rs:34600 | `emit_call` | `.find("____")` |
| 13 | emit_c.rs:35243 | `emit_call` | `if let Some(idx) = recv_type.find("____") {` |
| 14 | emit_c.rs:38028 | `emit_for` | `let iter_struct_base: String = iter_struct.split("____").next()` |
| 15 | emit_c.rs:38766 | `collect_pattern_inner_bindings` | `let sum_base = scr_base.split("____").next().unwrap_or("").to_string();` |
| 16 | emit_c.rs:40637 | `register_container_eq_mono` | `let base = match rt_trimmed.split_once("____") {` |
| 17 | emit_c.rs:43034 | `infer_mono_method_ret_with_args` | `let sep_pos = stripped.find("____")?;` |
| 18 | emit_c.rs:43920 | `register_novaopt_decl` | `let self_ref_mid_emission = c_ty.strip_prefix("Nova_")` |
| 19 | emit_c.rs:46220 | `fn_field_call_sig` | `.split("____")` |
| 20 | emit_c.rs:48610 | `infer_call_ret_c` | `let obj_bare = obj_bare.strip_prefix("Nova_").unwrap_or(obj_bare);` |
| 21 | emit_c.rs:49671 | `infer_expr_c_type` | `let base_name: &str = struct_name.split("____").next().unwrap_or(&struct_name);` |
| 22 | emit_c.rs:51115 | `infer_expr_c_type` | `let base_name: &str = struct_name.split("____").next().unwrap_or(&struct_name);` |

This 22/16 split is now the frozen `[M-196-raw-decode-allowlist]` baseline in
`compiler-codegen/tests/no_raw_type_decode.rs` (Ф.1a) — CI fails on any 23rd+ site, and the
test itself demands the allowlist SHRINK (stale-baseline assertion) as later phases close
sites, down to the empty-map close-out criterion.

## 2. Channel 6z catalog — `infer_expr_c_type`'s legacy match (Ф.1b deliverable)

`infer_expr_c_type` spans `emit_c.rs:48885`–`51499` (2615 lines). Channels 1-6y (lines
48885-50006) are the already-lifted, checker-integrated fast paths. **Channel 6z**
(`emit_c.rs:50007`-`51492`, tagged `[M-172.1-lifted-legacy-arms]` / `[P67-LEGACY]`) is the
"остаток legacy_inner перенесён ЦЕЛИКОМ" — one big `match &expr.kind { … }` starting at
`emit_c.rs:50166`, containing **44 explicit `ExprKind` arms + 1 wildcard** (recon's "~40"
confirmed as a reasonable rounding). Classification key:

- **A** — thin swap to `resolved_type_to_c(ir.type_of(expr))` is (or already is) trivial:
  either a syntax-fixed constant, a pure pass-through of an already-resolvable
  sub-expression's type, or (in one case) already primary-sourced from
  `self.resolved_types`. No new checker capability required.
- **B** — needs the CHECKER to annotate this `ExprId` first, using inference the checker
  already knows how to do in principle (Ф.1c literal/empty-sum annotation, or Ф.2
  non-primitive-Match/non-generic-RecordLit/TupleLit checker-extension, or equivalent
  "wiring" work) — no new engine required, just applying existing capability to more nodes.
- **C** — needs the Ф.4/172.13-Ф.3 mono/constraint-inference engine (the explicitly named
  "Binary-Join / If-Match-Join / resolve-семья" family, or generic-mono type-name
  computation in the same family as `[M-array-vec-unify]` Ф.5) — a genuinely new checker
  inference capability, not just applying an existing one to a new node.

| # | line | `ExprKind` | what it does | class | why |
|---|---|---|---|---|---|
| 1 | 50167 | `IntLit` | const `nova_int` | B | Ф.1c literal annotation |
| 2 | 50169 | `CharLit` | const `nova_char` | B | Ф.1c |
| 3 | 50170 | `FloatLit` | const `nova_f64` | B | Ф.1c |
| 4 | 50171 | `BoolLit` | const `nova_bool` | B | Ф.1c |
| 5 | 50172 | `StrLit` | const `nova_str` | B | Ф.1c |
| 6 | 50173 | `InterpolatedStr` | const `nova_str` | B | Ф.1c |
| 7 | 50174 | `UnitLit` | const `nova_unit` | B | Ф.1c |
| 8 | 50176 | `NullPtrLit` | const `void*` | B | Ф.1c |
| 9 | 50178 | `HexBlobLit` | const `Nova_Vec____nova_byte*` (D412) | B | Ф.1c (fixed literal type) |
| 10 | 50179 | `TupleLit` | recursive elem-type infer + `register_mono_tuple`/mono-name compute; erased-arity fallback | B (C for generic residual) | Ф.2 scope explicitly names non-generic TupleLit; generic case explicitly deferred ("generic — позже") |
| 11 | 50203 | `Binary` | cmp-ops→bool (trivial); arithmetic: value-record operator-overload dispatch, ptr arithmetic, typed-int promotion | **C** | "Binary-Join" family, explicitly Ф.4/172.13 Ф.3 |
| 12 | 50311 | `Unary` | `!`→bool, `-`→operand type, `&`/`*`→wrap/strip one `*` | A | pure mechanical transform of an already-resolvable operand type |
| 13 | 50339 | `Block` | trailing-expr type passthrough + legacy let-lookup workaround for stale-scope Ident trailing | A | thin once trailing sub-expr is covered; extra logic is scope-workaround, not new inference |
| 14 | 50394 | `RecordLit{Some name}` | sum-variant / generic-mono / value-record / heap-record dispatch from declared name | B (C for generic-mono branch) | Ф.2 explicit scope ("non-generic RecordLit… по прецеденту"); generic type-args-from-fields branch is mono-name computation |
| 15 | 50466 | `RecordLit{None}` | spread-source type passthrough | A | thin delegate |
| 16 | 50480 | `Ident` | override/var_types lookup, empty-sum-as-value, free-fn-value, **already falls back to `resolved_types`**, module-alias guard, panic | B | empty-sum branch = explicit Ф.1c scope; already closest to target pattern via its own fallback |
| 17 | 50583 | `Index` | slice passthrough, Vec-mono element decode (debt_ helpers + registries), str[i]→char, raw-ptr deref index | **C** | tied to `[M-array-vec-unify]` (Ф.5, flagged risky); not in Ф.1c/Ф.2 scope |
| 18 | 50708 | `SelfAccess` | receiver's own declared C-type (`var_types["nova_self"]`) | A | receiver type is always statically known from the method signature |
| 19 | 50746 | `HandlerLit` | mechanical `NovaVtable_<effect>*` from AST name | A | pure syntax→name formatting |
| 20 | 50752 | `ProtocolLit` | mechanical `NovaBox_<Proto>` from AST name | A | pure syntax→name formatting |
| 21 | 50757 | `Call` | delegates whole-hog to `infer_call_ret_c` (separate ~2592-line fn, `emit_c.rs:~48800`) | **C** | largest single arm; explicitly named in umbrella plan as needing the real mono engine |
| 22 | 50759 | `ArrayLit` | element-type infer (first item/spread/hint) + Vec[T] mono-name compute | **C** | same generic-mono family as RecordLit/TupleLit generic branches; overlaps `[M-array-vec-unify]` |
| 23 | 50828 | `If` | divergence-aware then/else join + unit-domination fallback (mirrors `emit_if_expr`) | **C** | the "If-…-Join" half of the named "Binary-Join/If-Match-Join" Ф.4 family |
| 24 | 50874 | `Match` | pattern-binding install, arm-type join, divergence-skip, Result-arm reconciliation, unit-domination | B | explicit Ф.2 scope ("non-primitive Match"); deeper generic-join residual is Ф.4 follow-on, not blocking |
| 25 | 51003 | `Member` | `@method`→void* (A); qualified `Type.Variant` value (B); size-accessor dead-guard (A); tuple `.N` via mono-name decode (C); newtype `.0` identity (A); record field-type lookup non-generic (B) / generic template+subst (C) | **mixed B/C** | genuinely composite arm — see per-branch notes; riskiest sub-logic (generic field subst, tuple `.N` decode) is C |
| 26 | 51173 | `Is` | const `nova_bool` | A | language invariant (`x is T` always bool) |
| 27 | 51174 | `As` | `expr as T` = lowered target `TypeRef` directly | A | already thin AST-type lowering, doesn't even consult operand |
| 28 | 51195 | `Range` | schema-presence-driven constant | A | module-config-driven, not per-expression inference |
| 29 | 51206 | `For` | const `nova_unit` | A | |
| 30 | 51207 | `ParallelFor` | trailing element-type infer + Vec mono-name compute | **C** | same Vec-mono family as `ArrayLit`/`Index` |
| 31 | 51224 | `While` | const `nova_unit` | A | |
| 32 | 51225 | `WhileLet` | const `nova_unit` | A | |
| 33 | 51226 | `Loop` | const `nova_unit` | A | |
| 34 | 51230 | `Supervised` | trailing-type passthrough + empty→unit normalize | A | |
| 35 | 51240 | `Detach` | const `nova_unit` | A | |
| 36 | 51243 | `Blocking` | trailing-type passthrough | A | |
| 37 | 51248 | `TaggedTemplate` | const `nova_str` | A | |
| 38 | 51251 | `Coalesce` | lhs-driven `NovaOpt_`/`NovaRes_` name-string unwrap, rhs fallback | B | standard `??`-join semantics; today's string-decode IS the "second window" pattern, but underlying inference is ordinary type-checking |
| 39 | 51277 | `With` | trailing-type passthrough + handler-binding probing (`Fail[E]` hint, Ok/Err reconciliation) | B | handler/effect binding types already checker-tracked; needs wiring not new engine |
| 40 | 51340 | `Try` \| `Bang` | same `NovaOpt_`/`NovaRes_` unwrap as Coalesce + panic | B | same reasoning as Coalesce |
| 41 | 51366 | `ClosureLight` | closure-struct synthesis; **params hardcoded to `nova_int`** (no real param-type inference) | **C** | genuine inference gap (closure param types), needs real capability, not wiring |
| 42 | 51400 | `IfLet` | **already** primary-sourced from `self.resolved_types` on both branches, empty-string fallback only | A | already the target end-state; only the fallback path awaits full coverage |
| 43 | 51434 | `Lambda` | deprecated (Plan 19) legacy syntax; explicit AST param types, explicit-or-inferred return | A | explicit type annotations already carry the answer; low priority (dead syntax) |
| 44 | 51449 | `Path` | qualified-name lookup across sum/record schema registries + var_types | B | same qualified-name-resolution family as `Ident`'s empty-sum branch |
| — | 51480 | `_` (wildcard) | unhandled-kind sentinel (empty string / debug-only panic) | N/A | no inference logic; its panic-never-firing IS the Ф.1d proof-dead-delete criterion |

### Tally

- **A** (thin swap, no new checker work): 19 arms — Unary, Block, RecordLit{None}, SelfAccess,
  HandlerLit, ProtocolLit, Is, As, Range, For, While, WhileLet, Loop, Supervised, Detach,
  Blocking, TaggedTemplate, IfLet, Lambda.
- **B** (needs checker-annotation wiring, Ф.1c/Ф.2-shaped): 17 arms — the 9 literals, TupleLit
  (non-generic), RecordLit{Some} (non-generic), Ident, Match, Coalesce, With, Try/Bang, Path.
- **C** (needs Ф.4/172.13-Ф.3 mono/constraint engine): 7 arms — Binary, Index, Call, ArrayLit,
  If, ParallelFor, ClosureLight.
- **mixed B/C**: 1 arm — Member (composite; generic/tuple sub-branches are C, the rest B/A).
- **N/A**: 1 wildcard.

44 arms + 1 wildcard = 45 rows, tally 19+17+7+1+1 = 45 ✓.

**Reading:** Ф.1c (literal+empty-sum annotation) directly retires the 9 literal arms + shrinks
`Ident`/`Path`'s B-residual. Ф.2 (non-primitive Match, non-generic RecordLit/TupleLit) retires
`Match` and the non-generic halves of `RecordLit{Some}`/`TupleLit`. The A-class arms (19 of 45)
are eligible for deletion via `resolved_type_to_c(ir.type_of(expr))` as soon as the checker's
per-`ExprId` coverage is complete enough that every A-arm's expression (or its sub-expression,
for passthrough arms) reliably carries a `resolved_types` entry — this is NOT blocked on Ф.1c/
Ф.2/Ф.4 checker work, only on `[M-104.10-expr-types-coverage]` reaching these specific nodes;
Ф.1d ("prove-dead→delete") is the mechanism that proves it safe per-arm via trace
instrumentation. The C-class arms (7, plus Member's generic residual) are gated on Ф.4 (the
172.13 Ф.3 constraint-inference engine) and cannot shrink before that lands. `Call`'s delegate
`infer_call_ret_c` (~2592 lines) remains the single largest piece of class-C debt.
