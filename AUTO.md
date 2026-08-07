# Авто-классификация остатка (интегратор, скрипт)

## B — самоописанная отложенность/ретракция
| E_DUPLICATE_POINTER_MODIFIER | B | . Extends `E_INVALID_POINTER_MODIFIER` (Plan 138.5 §1). - `E_DUPLICATE_POINTER_MODIFIER` — `*ro mut |
| E_LIT_PTR_NO_COERCE | B | as *()           // ok — explicit cast (Plan 134 / D214)     ro h *() = 0x1000 as *()      // ok — o |
| E_NULL_LITERAL_REPLACED_BY_OPTION | B | > > **После Plan 118 landed: `null ptr` полностью удаляется** — > retrac |
| E_NULL_LITERAL_USE_NONE | B | + init`; полноценный `MaybeUninit[T]` — Plan 118.2) |
| E_OUTER_MUT_IN_CONDITION | B | (`E_OUTER_MUT_IN_CONDITION`). - Chains (Plan 106) переиспользуют тот же `if_cond`. - Pattern grammar |
| E_PARSE_POINTER_TYPE_INCOMPLETE | B | . Extends `E_INVALID_POINTER_MODIFIER` (Plan 138.5 §1). - `E_DUPLICATE_POINTER_MODIFIER` — `*ro mut |
| E_POINTER_CROSS_FIBER | B | -невидимая-инфраструктура); > поглощает Plan 118.3 `#fiber_send`/`E_POINTER_CROSS_FIBER` (§5). > Мир |
| E_PTR_NO_MEMBER | B | `null T` где T ≠ ptr (V1 ограничение;   Plan 118 expand для `*T`). |
| E_REDUNDANT | B | 3.2 ordering (FLIPPED к safety-inner,   Plan 138.5) — value-T / pointee only - `[M-118.5-V3-redundan |
| E_REDUNDANT_POINTER_MODIFIER | B | dation for Nova-native data structures (Plan 131, 2026-06-08)     D232   02-types.md   Vec[T] — Nova |
| E_REENTRANT_CONDVAR_ERROR | B | ONDVAR_ERROR если   статически выявимо (Plan 103.4 + checker). |
| E_REF_MARKER_NOT_ALLOWED | B | size-driven авто-механизм Plan 172.4 (R3). Explicit `ro ref` — семантическая   аннотац |
| E_UNCHECKED_KIND | B | _KIND`.  #### Формат runtime-violation (Plan 140.1) |
| E_UNDEFINED_USE_NONE_INIT_PATTERN | B | + init`; полноценный `MaybeUninit[T]` — Plan 118.2) - Vararg calls — `E_VARARG_NOT_SUPPORTED` |
| E_UNSAFE_HANDLER_BUILTIN_ONLY | B | enforcement — D216 §20 + Plan 113 D172 V1 ENFORCED 2026-06-02   (commit 6752565f453) |
| E_VARARG_NOT_SUPPORTED | B | ### `unsafe fn` as part of fn-ptr type (Plan 118.1.6 closeout, 2026-06-08; amend Plan 118.1.7, 2026- |
| W_DEPRECATED_POINTER_INLINE_MODIFIER | B | orical (V2 grace-period draft, отозвано Plan 138.5):** ранее > планировался `W_DEPRECATED_POINTER_IN |
| W_DEVIRT_FAILED | B | 4. **From blanket mono** — extension Plan 101 mono pass на `fn[T] T.method`    static на generic |
| W_NON_CANONICAL_TYPE_MODIFIER_ORDER | B | ` (с machine-applicable fix-it), > а не отложенный lint `W_NON_CANONICAL_TYPE_MODIFIER_ORDER`. Полно |
| W_PTR_AS_INT_GC_HASH_HAZARD | B | action). Note: `usize`/`isize` removed (Plan 133) — use `int` for pointer-as-integer casts. |
| W_UNSAFE_GC_TRIGGER | B | snapshot + perf bench (A31, A32) >   - Plan 118.1/118.2/118.3 sub-plans |

## D — обрубки/заглушки
| E_LOCAL_ | D | обрубок/заглушка |
| E_PROTOCOL_EMBED_ | D | обрубок/заглушка |
| E_PTR_NO_DISPLAY_ | D | обрубок/заглушка |
| E_THREAD_AFFINE_ | D | обрубок/заглушка |
| E_UNSAFE_ | D | обрубок/заглушка |

## Требуют различения A (дыра) / C (под другим именем) — ручная работа
| E_CONST_FN_TRAMPOLINE_GENERIC | ? | 03-syntax.md:8219 |
| E_CONSUMED_AFTER_USE | ? | 06-concurrency.md:5954 |
| E_CONSUME_CROSS_FIBER | ? | 06-concurrency.md:5955 |
| E_CONSUME_NOT_CONSUMED | ? | 06-concurrency.md:5953 |
| E_DUPLICATE_LOCAL | ? | 03-syntax.md:8682 |
| E_DUP_DEFINITION | ? | 02-types.md:15356 |
| E_EQ_CYCLIC_TYPE | ? | open-questions.md:7687 |
| E_FIELD_NOT_MUT | ? | 02-types.md:12453 |
| E_FLUENT_SELF | ? | 03-syntax.md:6691 |
| E_GENERIC_CONST_CYCLE | ? | 02-types.md:8767 |
| E_GENERIC_CONST_REQUIRES_INSTANTIATION | ? | 02-types.md:8755 |
| E_LITERAL_COMPOSITION_NOT_ALLOWED | ? | 02-types.md:7374 |
| E_MATCH_EXTENSIBLE_NEEDS_WILDCARD | ? | open-questions.md:8440 |
| E_NO_FROM_IMPL | ? | 02-types.md:8192 |
| E_OVERLOAD_REF_AMBIGUOUS | ? | 10-overloading.md:328 |
| E_POINTER_RO_MUT_METHOD | ? | 02-types.md:9700 |
| E_PRIV_FIELD | ? | 08-runtime.md:969 |
| E_PRIV_FIELD_PROTOCOL | ? | 02-types.md:11378 |
| E_PRIV_TUPLE_POSITIONAL_ACCESS | ? | 02-types.md:11379 |
| E_PTR_ARITHMETIC_INVALID | ? | 02-types.md:9728 |
| E_PTR_WRITE_ON_RO_TARGET | ? | 02-types.md:9962 |
| E_REALTIME_VIOLATION | ? | 06-concurrency.md:4561 |
| E_REBIND | ? | 02-types.md:3374 |
| E_REF_MARKER_REQUIRED | ? | 09-tooling.md:3205 |
| E_REF_MODE_REQUIRES_RO_OR_MUT | ? | 09-tooling.md:3204 |
| E_RESERVED_WORD | ? | 02-types.md:2935 |
| E_UNDECLARED | ? | 09-tooling.md:3310 |
| W_BARE_UNLOCK_DEPRECATED | ? | 06-concurrency.md:5965 |
| W_D226_NEGATIVE_LITERAL | ? | 02-types.md:12284 |
| W_LOCAL_TOML_UNSUPPORTED_KEY | ? | 09-tooling.md:3770 |
| W_NARROW_ATOMIC_OVERFLOW_RISK | ? | 06-concurrency.md:4510 |
| W_SEMAPHORE_OVER_RELEASE | ? | 06-concurrency.md:5109 |
| W_SHADOW_UNRELATED | ? | 03-syntax.md:8663 |
| W_UNUSED_LOCAL | ? | 03-syntax.md:1123 |
| W_UNUSED_PARAM | ? | 03-syntax.md:1123 |
