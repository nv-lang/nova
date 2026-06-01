// SPDX-License-Identifier: MIT OR Apache-2.0
# Q-structural-extension-future — Future Direction (STUB)

> **Plan 110 Ф.14.2 Q-block (STUB).** Future direction: structural
> extension pattern `value.with_retry()`, `value.with_logging()`,
> `value.with_metrics()` via `T + Protocol` intersection types.
> Cross-ref [Plan 110 §«Rejected designs»](../../docs/plans/110-scoped-resources-radical-simplification.md#d190).

## Direction

Allow augmenting existing Consumable types with additional behavior
without inheritance OR modification:

```nova
// Future syntax (NOT в Plan 110 scope):
ro tx = db.begin()
    .with_retry(attempts: 3)        // intersection type Tx + Retryable
    .with_logging(LogLevel.Info)     // + Loggable
    .with_metrics(metrics)           // + Metricized

consume tx_augmented = tx {
    do_work()?
}
// On exit: Metricized.record + Loggable.flush + Retryable.cleanup +
// Consumable.on_exit (LIFO composition).
```

## Why Not в Plan 110

Intersection types (`T + P + Q + ...`) — **level-0 language feature**.
Plan 110 focused на cleanup-semantics simplification. Adding
intersection types simultaneously would:

1. Double scope (intersection types — own design space).
2. Mix concerns (cleanup vs general composition).
3. Risk premature design (intersection-pattern usage data not yet
   collected).

## Tentative Plan

- **Plan 112** (candidate): intersection type design + parser/AST/
  type-check support.
- **Plan 113** (candidate): `with_X(...)` method-chain syntax for
  type augmentation.

## Open Questions

1. Should intersection types support arbitrary composition OR be
   limited to protocol-protocol combinations?
2. How does method dispatch resolve when multiple components have
   same method name?
3. How does Consumable.on_exit order interact with composed protocols?
4. Should hot-path elision (D194) still work для augmented types?

## See also

- [D190 rejected designs](../../spec/decisions/03-syntax.md#d190) — alternative cleanup approaches.
- Plan 112 (placeholder).
- Plan 113 (placeholder).
