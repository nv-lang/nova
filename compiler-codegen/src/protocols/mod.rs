// Plan 126 (D230): auto-derive protocol synthesis infrastructure.
//
// Этот модуль группирует compiler-side auto-derive logic для built-in
// protocols (`Equatable`, `Hashable`, `Cloneable`, `Comparable`, `Printable`).
// Когда type помечен `#impl(P)` и НЕ предоставляет explicit `fn T @method`,
// synthesizer'ы создают memberwise рекурсивный AST FnDecl и регистрируют
// его в pipeline ((1) типовая таблица методов, (2) downstream codegen
// обрабатывает synthesized FnDecl как обычный user-defined).
//
// **Архитектура:**
// - `auto_derive` — core: AutoDeriveCtx + synthesize_method orchestrator.
// - Synthesis происходит во время type-check'а в `verify_impl_protocols`
//   через type-checker pass (Plan 91.9 D186 расширение).
// - Cycle detection — visited set по (type, protocol).
// - Field eligibility check — каждое поле должно быть либо primitive,
//   либо протокол-удовлетворяющим (`#impl(P)` или explicit method).
//
// **Cross-references:**
// - Plan 126 — auto-derive protocols.
// - D230 NEW — Cloneable protocol.
// - D186 — `#impl(P)` annotation.
// - D183 — protocol default bodies (separate mechanism, codegen-level).
// - D109 — Equatable / Hashable / Comparable / Printable.

pub mod auto_derive;
