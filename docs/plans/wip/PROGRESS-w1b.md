# PROGRESS — p-op-w1b (окно 196, операторный чекер-канал, спайк-продолжение)

Worktree: `d:/Sources/nv-lang/nova-pw1b`, branch `p-op-w1b`, база main `056c7a573`.
Модель: sonnet. Предыстория: `docs/plans/wip/196-op-channel-progress.md` (окно
p-op-channel) — карта + трёхшаговая рекомендация, дословно исполняемая этим
окном (`scratch38/BRIEF_op_channel_w1b.md`).

## Статус по шагам

| Шаг | Статус |
|---|---|
| 1 — резолв @minus/@shl/@shr на конкретном Nova_T* в чекер-канал | ✅ ГОТОВО — коммит `d72d194a4` |
| 2 — сравнения (==/!=/</<=/>/>=) через @equal/@compare | НЕ НАЧАТО — бюджет окна ушёл на шаг 1 + верификацию (см. ниже) |
| 3 — generic-mono/value-record/compound-assign | НЕ НАЧАТО (по плану — только после 1+2) |

## Шаг 1 — что сделано

- **Чекер** (`compiler-codegen/src/types/mod.rs`, `ExprKind::Binary`-арм,
  после блока `res_rt`/`resolved_types_buf`, ~9707): для `op ∈ {Sub, Shl,
  Shr}` (D46 Heterogeneous shape) резолвит receiver-тип левого операнда через
  `infer_expr_type` → `TypeRef::Named{path, generics}`; гейты: `path.len()==1
  && generics.is_empty()` (конкретный, не generic-mono) И `!is_primitive_recv_name`.
  Собирает `method_overloads(type_name, op_method)`, фильтрует кандидатов
  (`ReceiverKind::Instance`, `receiver.generics.is_empty()`, `f.generics.is_empty()`,
  ровно 1 non-variadic параметр, `assignable(right, &param.ty, ...)` не `Bad`);
  при РОВНО ОДНОМ совпадении пишет `resolved_callees.insert(e.id, single.span)`.
  Result-type inference (`res_rt`) НЕ тронут — блок чисто аддитивный, добавлен
  ПОСЛЕ существующей логики (audit POISON 6875 / D263 guard соблюдён).
- **Codegen** (`compiler-codegen/src/codegen/operator_dispatch.rs`):
  `resolve_binop_dispatch` получил новый параметр `channel_callee:
  Option<Span>` — если задан и совпадает по `fn_span` с одной из `overloads`,
  возвращает `BinOpResolution::Concrete` НАПРЯМУЮ, минуя весь rty-текстовый
  скан ниже (комментарий-doc на функции). Вызывающая сторона в `emit_c.rs`
  (~34473, `Nova_T*`-путь) передаёт `self.resolved_callees.get(&expr.id).copied()`
  как последний аргумент — правка вписана В СУЩЕСТВУЮЩУЮ строку аргументов
  (`overloads.as_deref(), mono_fn_decl, self.resolved_callees...`), рост
  `emit_c.rs` = 0 строк (ratchet считает только `emit_c.rs`, не
  `operator_dispatch.rs` — вся новая логика туда и перенесена).
  Fallback (канал пуст — Homogeneous-операторы никогда его не пишут,
  generic-mono ресиверы, сравнения, любой producer-gap) — прежний путь без
  изменений (strangler-fig, kill-switch не нужен).

### Байт-parity / гейты (все подтверждены)

- `nova check std/src` → `PASS: 147 FAIL: 26 WARN: 60` — байт-в-байт с
  baseline, подтверждено ДО и ПОСЛЕ обеих правок (чекер отдельно, потом
  codegen).
- `scripts/guards/arch-ratchet.sh` → `lines=64311 <= 64311` (без роста),
  `infer=348 <= 349` (без роста). Первая версия правки (канал читался
  инлайн в `emit_c.rs` отдельным блоком) дала `lines=64313`/`64330` —
  ОБЕ провалили ратчет; логика перенесена в `operator_dispatch.rs`,
  вызов в `emit_c.rs` сведён к одной строке аргументов → 0 роста.
- `cargo test --release --manifest-path compiler-codegen/Cargo.toml --lib
  operator_dispatch` → **7/7 PASS** (6 существовавших + 1 новый:
  `resolve_heterogeneous_channel_callee_wins_over_rty_mismatch` — строит
  два `MethodSig` с разными `fn_span`, доказывает, что канал выбирает ИМЕННО
  тот, чей span совпал, ДАЖЕ когда `rty`-строка не совпала бы ни с одним —
  подтверждает, что канал реально приоритетен, а не мёртвый код).
  Note: `--lib`-фильтр использован намеренно — полный `cargo test
  operator_dispatch` без `--lib` тянет ~40 несвязанных integration-test
  бинарников этого же крейта (имя фильтруется по подстроке видимого
  test-файла, а не по тесту), что на этой машине под конкурентной нагрузкой
  (см. ниже) периодически ловило `LINK: LNK1105 не могу закрыть файл`
  (антивирус-гонка/файл-лок, НЕ ошибка кода) — `--lib` даёт тот же
  7/7-результат без сборки посторонних бинарников.
- **16/16 операторных conformance-фикстур** (`nova test`, `--toolchain
  clang`, worktree-env): `d215_named_tuple_{eq,fluent,value}`,
  `d363_operator_dispatch_protocols`, `m234_{bitnot_pos,bitwise_rename_pos,
  compound_assign_pos,set_bitor_generic_mono}`, `m247_named_tuple_compare_ops`,
  `neg/{d46_not_custom_type_neg, m234_bitnot_bool_neg, m234_bitnot_f64_neg,
  m234_bitnot_str_neg, m234_neg_bool_neg, m234_neg_str_neg, m234_not_int_neg}`
  → `PASS: 16 FAIL: 0`.
  **Честная оговорка**: этот набор в основном покрывает bitwise/tuple-compare
  семьи, НЕ прямое попадание в новый Sub/Shl/Shr-путь (только `d363`
  потенциально пересекается — не проверено построчно). Реально бьющие в
  канал фикстуры — `m128_shl_shr_user_type_pos.nv` (concrete `M128Mask
  @shl`/`@shr` + generic-mono `M128GenBits[T]` контроль),
  `neg/m128_shl_shr_no_overload_neg.nv` (D124 no-matching-overload на
  Heterogeneous, канал ДОЛЖЕН промолчать — 0 кандидатов), `m_opu_arith_
  generic_mono.nv` (`@minus` на generic-mono, канал ДОЛЖЕН промолчать —
  `generics.is_empty()`-гейт исключает), `neg/d124_monotonic_minus_
  timestamp_neg.nv` (Monotonic — value-record ABI, не `Nova_T*`-путь,
  канал безвреден даже если чекер что-то запишет — codegen его не читает
  вне `is_single_nova_ptr`-ветки), `d318_monotonic_non_regression.nv`.
  Эти 5 файлов ЗАПУЩЕНЫ (`nova test --jobs 4`), но результат НЕ дождался
  подтверждения в рамках этого окна — машина под жёсткой CPU-конкуренцией
  (см. ниже), процесс завис дольше разумного бюджета верификации; окно
  закрывается БЕЗ подтверждённого вердикта по этой супplementary-проверке.
  Это НЕ блокер приёмки шага 1 — все обязательные по брифу гейты (check
  std/src, ratchet, cargo test operator_dispatch, 15+ операторных фикстур)
  зелёные и уже покрывают byte-parity/regression-риск; данный 5-файловый
  набор был ДОПОЛНИТЕЛЬНОЙ (не обязательной по брифу) проверкой, что канал
  реально «стреляет» на живом Sub/Shl/Shr-коде, не только в юнит-тесте.
  **Followup для следующего окна**: подтвердить `m128_shl_shr_user_type_pos.nv`
  + `neg/m128_shl_shr_no_overload_neg.nv` явно (быстрый прогон, когда машина
  не под конкурентной нагрузкой) — если это НЕ будет сделано до влития,
  считать шаг 1 верифицированным ТОЛЬКО юнит-тестом
  (`resolve_heterogeneous_channel_callee_wins_over_rty_mismatch`) +
  инвариантными gate'ами, без live conformance-подтверждения на самом
  Heterogeneous-семействе.

### Экологическая заметка (для следующих окон в этом worktree)

Машина в течение этой сессии несла ОДНОВРЕМЕННО ≥2 тяжёлых `nova test`
процесса из ГЛАВНОГО репо (`d:/Sources/nv-lang/nova`, другая параллельная
сессия/интегратор: `nova test src --strict-effects`,
`nova test --positive --compile-error spec_tests/conformance`) — это НЕ
мои процессы (правило «≤2 nova-процессов» соблюдено на СВОЕЙ стороне,
конкуренция пришла извне). Следствие: единичные `test-build`/`test`
вызовы, обычно секунды, растягивались до **40+ минут** на файл (CPU
starvation, не баг). `kill -0 <winpid>` в git-bash/MSYS НЕНАДЁЖЕН для
процессов, запущенных напрямую как Win32 exe (не через MSYS fork) —
возвращает "процесс не найден" даже когда процесс жив (подтверждено:
`kill -0 69668` вернул false-«умер» на процессе, который PowerShell
`Get-CimInstance Win32_Process` показывал ещё живым). Для ожидания
конкретного нативного PID в этом окружении надёжнее `Wait-Process -Id
<pid>` (PowerShell) или polling `Get-Process -Id <pid> -ErrorAction
SilentlyContinue`, не bash `kill -0`.

## Шаги 2/3 — почему не начаты

Бюджет окна ушёл на: (а) собственно шаг 1 (реализация + 2 итерации на
ratchet-гейт — первая версия правки годно работала, но росла на emit_c.rs,
потребовался перенос логики в operator_dispatch.rs); (б) верификацию под
неожиданно тяжёлой конкурентной нагрузкой (16-файловый conformance-прогон
занял ~42 минуты вместо ожидаемых секунд — CPU starvation от параллельной
сессии в главном репо, не связано с кодом). Разведка шага 2 частично
сделана (прочитан `emit_c.rs` 34494-34546 — `@equal`/`@compare`-protocol-
chain, `==`/`!=` сначала ищут `method_overloads(type, "equal")` с ЕДИНСТВЕННЫМ
instance-overload'ом без rty-матчинга, затем synthesis через `@compare`
если `equal` нет; `<`/`<=`/`>`/`>=` — ТОЛЬКО через `@compare`, если тип
`Comparable`): вывод — это структурно ДРУГАЯ задача (бинарный выбор
"equal-vs-compare-synthesis", не "какой из N overload'ов"), не сложнее
шага 1, но требует отдельной итерации гейтов — оставлено следующему окну
как рекомендовал брифинг ("честный стоп — приемлемый исход").

## Коммиты (чекпоинты)

- `d72d194a4` — feat(196/p-op-w1b): checker-channel callee resolve for
  Heterogeneous binops (spike step 1)
