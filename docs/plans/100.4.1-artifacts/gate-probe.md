// SPDX-License-Identifier: MIT OR Apache-2.0
# Plan 100.4.1 — Ф.0 GATE: design probe + Plan 49 infrastructure audit

> Дата: 2026-05-26. Worktree: `nova-p100-4-1-failable`. Branch: `plan-100-4-1-failable-cleanup`.
> HEAD baseline: `1ac5e1cdce0`.

---

## 1. Design D1–D7 locked

Все 7 design decisions из [100.4.1-failable-cleanup-body.md](../100.4.1-failable-cleanup-body.md):
- **D1** — Fail effect разрешён в defer/errdefer body (amend D90 §4).
- **D2** — Composition rules (3 сценария: normal-exit-fail / propagation+defer-fail / multi-defer).
- **D3** — Composition primitive: Plan 49 + новый `nv_compose_suppressed`.
- **D4** — Defer-fail-on-normal-exit семантика (fn-sig обязан Fail[E]).
- **D5** — Type-checker: defer body Fail propagation.
- **D6** — Async-friendly composition (без новых async-mechanism'ов).
- **D7** — Diagnostic / AI-first ergonomics.

Подтверждено: design устойчив, нет открытых вопросов design-уровня.

---

## 2. Plan 49 infrastructure audit

### Что есть (`compiler-codegen/nova_rt/effects.h`):

```c
typedef struct NovaFailFrame {
    jmp_buf            jmp;
    nova_str           error_msg;
    NovaThrowKind      error_kind;          // USER / CANCEL / USER_TYPED
    void*              error_reason_ptr;    // Plan 49 typed cancel
    void*              error_user_payload;  // Plan 61 typed user payload
    NovaTypeId         error_user_type_id;  // Plan 61
    struct NovaFailFrame* prev;
} NovaFailFrame;
```

- `nova_throw(msg)`, `nova_throw_cancel(msg)`, `nova_throw_typed(msg, payload, tid)` — fill `_nova_fail_top` + longjmp.
- `Nova_Fail_fail` — user-handler dispatch (D65 strict).
- `NOVA_TRY(f) / NOVA_CATCH(f) / NOVA_CATCH_KIND(f)` — setjmp wrapper macros.

### Что НЕТ:

- **Suppressed-chain** в `NovaFailFrame`: нет поля для append'а вторичных ошибок.
- **`nv_compose_suppressed(primary, secondary)`**: нет helper'а.
- **Defer-body Fail-frame**: текущий emit_defer (см. Plan 20 Ф.8 (2)) кладёт NovaFailFrame только для errdefer на throw-path; failable defer body — не поддерживается.
- **MultiError type**: нет.

### Заключение

Plan 49 infrastructure покрывает **single-error throw routing** (kinded + typed + user payload). Для composition нужно:
1. Расширить `NovaFailFrame` полем `NovaErrorChain* error_suppressed` (head of linked list).
2. Добавить `nv_compose_suppressed(primary_frame, msg, kind, payload, tid)` helper.
3. emit_defer для тел с potentially-Fail effect эмитит свой `NovaFailFrame`, и в case setjmp returns non-zero — либо direct throw (normal exit), либо `nv_compose_suppressed` в outer's chain (during unwinding).

---

## 3. Три сценария — handwritten verification

### Сценарий A: defer-fail на normal exit

**Nova:**
```nova
fn process() Fail[CommitErr] -> () {
    consume tx = begin()
    defer { tx.commit() }   // fail'абельный commit
    do_work()               // success
}
```

**Generated C (логика):**
```c
nova_unit process(void) {
    NovaFailFrame _fn_frame; nova_fail_push(&_fn_frame);
    if (setjmp(_fn_frame.jmp) != 0) { nova_fail_pop(); nova_throw(_fn_frame.error_msg); }

    Transaction tx = begin();
    int _did_throw = 0;

    do_work();   // success path

    // === defer cleanup at scope exit ===
    NovaFailFrame _defer_frame; nova_fail_push(&_defer_frame);
    if (setjmp(_defer_frame.jmp) == 0) {
        Transaction_commit(tx);      // may longjmp into _defer_frame
        nova_fail_pop();             // happy path
    } else {
        nova_fail_pop();
        // No outer unwinding active → re-throw из defer body как fresh fail.
        nova_throw_typed(_defer_frame.error_msg,
                         _defer_frame.error_user_payload,
                         _defer_frame.error_user_type_id);
    }

    nova_fail_pop();    // pop _fn_frame
    return NOVA_UNIT;
}
```

`_did_throw` нужен в сценарии B (distinguish normal exit vs unwinding).

**Verdict A:** ✅ Работает с расширенным emit_defer; никаких новых runtime helpers (только NovaFailFrame wrap для defer body).

### Сценарий B: defer-fail во время error propagation

**Nova:**
```nova
fn process() Fail[Err] -> () {
    consume tx = begin()
    defer { tx.commit() }
    do_work_that_fails()?           // throws Err1
    // unwinding starts; defer fires → commit fails Err2
    // composite: { primary: Err1, suppressed: [Err2] }
}
```

**Generated C (логика):**
```c
nova_unit process(void) {
    NovaFailFrame _fn_frame; nova_fail_push(&_fn_frame);
    if (setjmp(_fn_frame.jmp) != 0) {
        nova_fail_pop();
        // _fn_frame.error_msg + suppressed populated by defer chain.
        // Re-throw upward preserving suppressed:
        nova_rethrow_with_suppressed(&_fn_frame);
        // unreachable
    }

    Transaction tx = begin();
    int _did_throw = 0;

    NOVA_TRY(/* implicit through codegen */) {
        do_work_that_fails();  // longjmps; this code never reached
    }

    // do_work_that_fails throws → control jumps to nearest fail-frame.
    // BUT we want defer to run BEFORE the throw escapes our scope.

    // The actual codegen pattern: wrap statement-level with intra-scope
    // try-blocks AND emit scope-exit cleanup в LIFO order. На throw:
    // 1. setjmp catch at _fn_frame.
    // 2. _did_throw = 1.
    // 3. Run all defer'ы scope'а с _did_throw flag.
    // 4. После каждого defer: если он сам longjmp'нул → append в _fn_frame.error_suppressed.
    // 5. После всех defer'ов → longjmp upward с _fn_frame as primary.

    // === defer cleanup (called from cleanup path) ===
    NovaFailFrame _defer_frame; nova_fail_push(&_defer_frame);
    if (setjmp(_defer_frame.jmp) == 0) {
        Transaction_commit(tx);
        nova_fail_pop();
    } else {
        nova_fail_pop();
        if (_did_throw) {
            // Append to primary (_fn_frame) suppressed chain.
            nv_compose_suppressed(&_fn_frame,
                                   _defer_frame.error_msg,
                                   _defer_frame.error_kind,
                                   _defer_frame.error_user_payload,
                                   _defer_frame.error_user_type_id);
            // Continue unwinding (next defer or exit scope).
        } else {
            // Normal-exit failure — re-throw (Сценарий A behaviour).
            nova_throw_typed(_defer_frame.error_msg, ..., ...);
        }
    }

    if (_did_throw) {
        nova_rethrow_with_suppressed(&_fn_frame);  // resumes propagation
    }

    nova_fail_pop();
    return NOVA_UNIT;
}
```

`nv_compose_suppressed` API:
```c
typedef struct NovaErrorChain {
    nova_str           msg;
    NovaThrowKind      kind;
    void*              user_payload;
    NovaTypeId         user_type_id;
    struct NovaErrorChain* next;
} NovaErrorChain;

static inline void nv_compose_suppressed(NovaFailFrame* primary,
                                          nova_str msg,
                                          NovaThrowKind kind,
                                          void* payload,
                                          NovaTypeId tid) {
    NovaErrorChain* node = (NovaErrorChain*)nova_alloc(sizeof(NovaErrorChain));
    node->msg = msg;
    node->kind = kind;
    node->user_payload = payload;
    node->user_type_id = tid;
    node->next = primary->error_suppressed;
    primary->error_suppressed = node;
}

static inline void nova_rethrow_with_suppressed(NovaFailFrame* frame) {
    // Frame already popped. Push values into nearest outer fail-frame,
    // copying suppressed chain pointer (chain owned by GC).
    if (_nova_fail_top) {
        _nova_fail_top->error_msg = frame->error_msg;
        _nova_fail_top->error_kind = frame->error_kind;
        _nova_fail_top->error_user_payload = frame->error_user_payload;
        _nova_fail_top->error_user_type_id = frame->error_user_type_id;
        _nova_fail_top->error_suppressed = frame->error_suppressed;
        longjmp(_nova_fail_top->jmp, 1);
    }
    // No outer handler — abort с full chain dump.
    nova_abort_with_chain(frame);
}
```

**Verdict B:** ✅ Работает с (NovaErrorChain + nv_compose_suppressed + nova_rethrow_with_suppressed) — три новых helper'а в `effects.h`.

### Сценарий C: Multiple defers, each can fail

**Nova:**
```nova
fn process() Fail[Err] -> () {
    consume tx1 = begin()
    consume tx2 = begin()
    defer { tx2.commit() }   // LIFO last → fires первым
    defer { tx1.commit() }   // LIFO first → fires вторым
    do_work()?                // throws Err1
    // unwind: tx1.commit() fails → append Err2 to chain; продолжаем
    //         tx2.commit() fails → append Err3 to chain; продолжаем
    // composite: { primary: Err1, suppressed: [Err2, Err3] }
}
```

**Codegen extension:** LIFO defer-loop с individual NovaFailFrame на каждый defer. После каждого defer: если fail'нул и `_did_throw == 1` → `nv_compose_suppressed`; продолжаем loop (не abort, не break).

Plan 100.4.4 (D161) более детально проработает multi-defer error accumulation. Для 100.4.1 достаточно базового LIFO-with-compose; **продолжение despite-failure** уже работает (defer-loop не stop'ит на individual fail).

**Verdict C:** ✅ Работает с тем же набором helper'ов, без новых.

---

## 4. Implementation surfaces

### Runtime (`compiler-codegen/nova_rt/effects.h`)
- Расширить `NovaFailFrame` полем `NovaErrorChain* error_suppressed`.
- Новый тип `NovaErrorChain` (singly-linked list).
- Helper'ы: `nv_compose_suppressed`, `nova_rethrow_with_suppressed`, `nova_abort_with_chain`.
- API: `nova_failframe_primary_msg(frame)`, `nova_failframe_suppressed_count(frame)`, `nova_failframe_suppressed_at(frame, i)`.

### Type-checker (`compiler-codegen/src/types/mod.rs`)
- `check_defer_body_inner`: разрешить `Throw` / `Try` / `Bang` если enclosing fn-sig имеет совместимый `Fail[E]`. Иначе error D158-defer-fail-not-in-sig.
- Также: `Call` arm — нужно ловить call'ы которые имеют Fail effect (не только syntax-level). Текущая логика проверяет только suspend-effects; расширяем на Fail.
- `Interrupt` — остаётся error всегда (это hijack scope exit, не failable cleanup).
- Передать `fn_sig_effects: &[TypeRef]` (Fail-types в enclosing fn) как ctx параметр.

### Codegen (`compiler-codegen/src/codegen/emit_c.rs`)
- emit_defer для тел с potentially-Fail effect: обернуть в `NovaFailFrame` setjmp. На non-zero return:
  - Если `_did_throw == 0` (normal exit path) → `nova_throw_typed` (fresh propagation).
  - Если `_did_throw == 1` (unwinding) → `nv_compose_suppressed(outer_frame, ...)`.
- Aналогично для `errdefer` (fires только на throw-path → всегда `nv_compose_suppressed`).
- Detect "potentially-Fail body" во время codegen — query `expr_has_fail_effect(body, fn_effects)` (re-using existing effect-walker).

### MultiError prelude (`std/prelude/multi_error.nv` или подобное)
```nova
type MultiError {
    primary_msg: str,
    suppressed: []str,
}

fn MultiError @primary() -> str => self.primary_msg
fn MultiError @suppressed() -> []str => self.suppressed
fn MultiError @fmt_chain() -> str => ...
```

Auto-construction: при catch (codegen для `match expr { Err(e) => ... }`) — если `_nova_fail_top->error_suppressed != NULL`, materialize MultiError; иначе обычный `e` (existing behaviour).

---

## 5. Open questions / risks

### 5.1 GC ownership NovaErrorChain
Chain nodes аллоцируются через `nova_alloc` (GC-managed). NovaFailFrame stack-allocated; `error_suppressed` head pointer — heap. После longjmp + propagation pointer переноситcя в outer NovaFailFrame. **OK:** GC sees the chain through outer frame's `error_suppressed` field. На abort — chain dropped, GC reclaims.

### 5.2 Re-throw transfer of chain
`nova_rethrow_with_suppressed` копирует pointer chain head в outer frame. Это работает для linear unwinding (single-thread, no fiber crossings). Для cross-fiber propagation (если возможно) — extra audit нужен; но Plan 49 ✅ для cross-fiber routing уже использует `nova_throw` который не несёт chain. Для 100.4.1 ограничиваемся sync + same-fiber; cross-fiber composition outside scope (deferred to 100.4.2 / D159).

### 5.3 Memory pressure при many failures
Composite-error с 1000+ suppressed — теоретически возможен (long-running loop с failable cleanup в каждой iteration). Plan 100.4.4 (D161) добавит cap (например, max 100 suppressed; остальное counted "and N more"). Для 100.4.1 — без cap (production'ом deal with в 100.4.4).

### 5.4 Interaction с handler-wrap backward-compat
Existing handler-wrap code:
```nova
defer {
    with Fail = handler { fail(e) { Log.error(e); interrupt () } } { risky() }
}
```
— продолжает работать. Inner `with Fail` ловит Fail; `interrupt ()` exits inner with-block; defer body normal exit. Никакого composition, никакого suppression — old behaviour. ✅

---

## 6. Acceptance Ф.0

- [x] D1-D7 locked.
- [x] Plan 49 infrastructure audit — chain/compose missing, нужно добавить.
- [x] 3 сценария hand-verified — design covers.
- [x] Implementation surfaces enumerated.
- [x] Open questions / risks documented.

**GATE Ф.0: PASS.** Переходим к Ф.1 spec amend.
