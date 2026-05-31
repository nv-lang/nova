// SPDX-License-Identifier: MIT OR Apache-2.0
# Plan 110.4 Progress Summary — MultiError + Cleanup + Application

> **Plan 110.4 partial closure documentation.** 4/8 sub-sub done +
> T8.1 fixture. Remaining work documented для следующих sessions.

## Status

| Sub-sub | Status | Commit |
|---|---|---|
| 110.4.1 MultiError @walk + @find_first_panic API | ✅ landed | 0f095e9d438 |
| 110.4.2 MultiError cycle-safety + depth-limit 256 | ✅ landed | 2b4a90e5569 |
| 110.4.3 Cleanup effect declaration | ✅ landed | 7188bab69fb |
| 110.4.4 Cleanup effect codegen emit (handler dispatch) | 🔴 DEFERRED | substantive effect-runtime work |
| 110.4.5 Application effect declaration | ✅ landed | 7188bab69fb |
| 110.4.5 T8.1 — basic operations type-check | ✅ landed | f040ad951e9 |
| 110.4.6 Application Level-2 integration | 🔴 DEFERRED | depends on Plan 110.2 nv_resolve_exit_timeout |
| 110.4.7 Cross-fiber propagation via D80 | 🔴 DEFERRED | depends on Plan 110.4.5/4.6 runtime impl |
| 110.4.8 Close phase | 🟢 THIS DOCUMENT | — |

## What's Done

### MultiError (110.4.1 + 110.4.2)

**API additions** (`std/prelude/errors.nv`):
- `MultiError @walk() -> []str` — LIFO chain iterator.
- `MultiError @find_first_panic() -> Option[str]` — panic detection
  via prefix matching.

**Cycle-safety** (`nova_rt/effects.h`):
- `nv_compose_suppressed` extended с identity check (prevents
  Java JDK-8287921 cycle bug).
- Depth-limit 256 enforced — silently no-op после reach.

### Cleanup + Application Effect Declarations (110.4.3 + 110.4.5)

`std/prelude/effects.nv` declarations:
- `Cleanup` effect: `on_scope_enter(label, timeout_ms)` + `on_scope_exit(label, outcome)`.
- `Application` effect: `register_finalizer(f)` + `default_exit_timeout_ms()`.

Type-check + compile verified via T8.1 fixture.

## What's Deferred

### 110.4.4 — Cleanup effect codegen emit

Codegen ConsumeScope должен emit `perform Cleanup.on_scope_enter` /
`on_scope_exit` calls when Cleanup handler bound. Requires:
1. Detection of bound Cleanup handler (effect-stack check).
2. Emit perform-effect call with proper arguments.
3. Handler default no-op recognition for elision.
4. OpenTelemetry reference impl in stdlib (per D185 §otel).

**Dependency:** None (independent от Plan 110.2 cancel-shield work).
**Estimated:** ~1-2 days for codegen integration + OTel example.

### 110.4.6 — Application Level-2 integration

`nv_resolve_exit_timeout` runtime function (per D192 3-level fallback)
needs Application effect Level-2 check. Depends on:
- Plan 110.2 cancel-shield runtime (where `nv_resolve_exit_timeout`
  is implemented).
- Application handler runtime impl (creation/destruction lifecycle).

**Estimated:** integrated with Plan 110.2 (~1-2 weeks combined).

### 110.4.7 — Cross-fiber propagation

D195 R6: spawn child fiber sees parent's Application via D80 effect
snapshot. Requires:
1. D80 effect snapshot machinery in spawn codepath.
2. Application handler lifetime extension (refcount keeping alive
   до last fiber).
3. Concurrent access safety to finalizer registry.

**Dependency:** Plan 110.4.5 runtime impl + spawn integration.
**Estimated:** ~2-3 days after 110.4.5 impl.

## Documentation Coverage

Q-blocks landed in Plan 110.8.1:
- Q-application-effect: full D195 nesting semantics (R1-R8) documented.
- Q-debugging-cleanup-chains: MultiError walk + OTel tracing recipes.
- Q-perf-considerations: Cleanup effect OTel cost (1-2 μs per scope).

## Connection to Other Sub-plans

- **Plan 110.1.4.f throw re-raise** uses MultiError compose path
  (cycle-safety from 110.4.2).
- **Plan 110.1.4.g panic discrimination** uses NOVA_THROW_PANIC kind;
  panic captured as ScopeOutcome::Panic variant.
- **Plan 110.2 cancel-shield** depends on Application handler Level-2
  integration (110.4.6).
- **Plan 110.5.6 cancel-as-CancelError** routes через Failure variant
  with "cancel: " prefix.

## Hard Blockers for Closure

110.4.4 + 110.4.6 + 110.4.7 — for full A9 (Cleanup effect) + A10
(Application effect + finalizers + default_exit_timeout + nesting +
spawn propagation) acceptance criteria. Currently A9/A10 partial:
declarations done, runtime integration deferred.

## See also

- [Plan 110 umbrella](../110-scoped-resources-radical-simplification.md).
- [Plan 110 decomposition](decomposition.md).
- [Q-application-effect](../../idiom/application-effect.md).
- [Q-debugging-cleanup-chains](../../idiom/debugging-cleanup-chains.md).
- [Plan 110.1 close summary](plan110_1_close_summary.md).
