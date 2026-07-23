# Plan 224 — L1-ro launder via mut-binding: закрыть [M-ro-launder-via-mut-binding]

**Статус:** Ф.0б/Ф.1/Ф.2 (std+examples+spec_tests) СДЕЛАНЫ и провалидированы
(gates §6 зелёные). nova-http — только инвентарь (не мой мандат, отдельная волна).

## §0. Контекст

Маркер [M-ro-launder-via-mut-binding] (`docs/plans/backlog-followups.md`):
L1-ro источник (параметр по D176-дефолту ЛИБО явный `ro`-локал, P7 freeze)
был полностью невидим для coercion-проверки — coercion проверялся только по
оси L2 (тип-модификатор `ro T`), никогда по биндингу источника (L1). Дыра
позволяла `mut b = a` (или передачу `a` аргументом в `mut`-параметр, или
`return a` из функции) свободно "отмыть" любое ro-связанное кучевое значение
в mutable-контекст, и запись через новый путь была видна оригиналу/вызывающему.

**Не баг против действующей спеки** — компилятор буквально соблюдал старую
формулировку P8 («coercion по оси content (L2), независимо от L1»). Дыра — в
самой спеке: P7 морозит owned-граф через биндинг, P8 при переносе в другой
биндинг игнорирует L1 → заморозка не переживает ре-биндинг/аргумент/возврат.
Закрытие — язык-изменение, амендмент D246 в этом же слиянии (сделан, §2).

**Норма — ФИНАЛЬНАЯ редакция (владелец, 2026-07-23, дважды подтверждена в
процессе волны):** L1-ro источник в mut-цель — `E_READONLY_COERCE` во ВСЕХ
трёх позициях (инициализация биндинга · аргумент вызова · возврат), для ВСЕХ
типов, **КРОМЕ голых скалярных примитивов** (`int`/`i8`…`i64`/`u8`…`u64`/
`f32`/`f64`/`bool`/`char`/`byte` — типы БЕЗ полей вообще: копия независима by
construction, рекурсивная хрупкость value-record-класса — проба G — к ним не
относится). Value-record (даже БЕЗ кучевых полей, проба E) остаётся ❌ —
классовое послабление по value-record ОТКЛОНЕНО. Дверь для независимой копии
любого не-скалярного типа — явный `.clone()` (D230). Диагностика ведёт
подсказками в порядке (a) `mut`-параметр/локал (in-out, D326-ревизия §Р3) →
(b) `.clone()` (с одностроч-обоснованием, если (a) не подходит) → (c) `ro`-цель.

## §1. Фазы — итог

| Фаза | Статус |
|---|---|
| Ф.0 (разведка) | Точный до-кода grep-инвентарь оказался ненадёжен; реальные числа сняты ЧЕРЕЗ сам чекер (`nova check`) после Ф.1 — см. §5 (история находки) |
| Ф.0б (D246-амендмент) | ✅ СДЕЛАНО |
| Ф.1 (чекер, 3 позиции) | ✅ СДЕЛАНО (init/call-arg/return) |
| Ф.2 (миграция std/src+examples+spec_tests) | ✅ СДЕЛАНО — 0 `E_READONLY_COERCE` во всех трёх после миграции |
| Ф.2 (nova-http) | НЕ мигрировано (чужая репа, только инвентарь — 345 хитов на момент замера, ждёт волны у мейнтейнера) |
| Гейты (§6) | ✅ зелёные (см. дословные вердикты) |

## §2. D246-амендмент — что именно записано

`spec/decisions/02-types.md`, секция `## D246`:
- Status-баннер: `AMENDED 2026-07-23` со ссылкой на этот план.
- P8 (в «10 принципов»): retract «независимо от L1» + explicit граница
  скаляр-примитив ≠ value-запись (проба E остаётся neg).
- Новый ORACLE **G** (cross-binding re-init) и **H** (cross-binding
  call-argument) — буквы продолжают ПОСЛЕ уже существующего
  `#### ORACLE F` (Ф.7 heading), не переиспользуют её; ORACLE G текст incl.
  явное исключение для скаляров.
- Новая секция «Таблица конверсий между binding'ами (L1,L2)источник →
  (L1,L2)цель» — с отдельной строкой-исключением для скаляр-примитивов + сноска
  про split (не даёт новых прав, но норма всё равно отклоняет перенос).
- Отдельный абзац «Диагностика (порядок подсказок)» — фиксирует канон
  mut-параметр-первым, `.clone()`-вторым.
- `E_READONLY_COERCE` bullet расширен (L1-ось + возврат-позиция).
- Cross-amend impact + Связь — ссылка на этот план и на D230/D326.

## §3. Ф.1 — что реализовано в чекере

Код: `compiler-codegen/src/types/mod.rs`.

1. **Let-инициализация (аннотированная И неаннотированная).**
   `TypeCheckCtx::check_readonly_source_coerce` (рядом с `f1_check_assign_let`)
   проверяет L2 (существовавшее) И L1 (новое — RHS-Ident в `ro_binding_names`,
   МИНУС scalar-primitive exemption). Новая `else`-ветка в `f1_stmt`'s
   `Stmt::Let` покрывает НЕаннотированный `let` — раньше annotation была
   обязательным условием срабатывания (корень дыр A/D/F/G).
2. **Call-argument (free-fn и метод).** `ConsumeCtx.readonly_locals` расширен
   L1-ro параметрами (`!effective_mut`, EXCLUDING `is_variadic` — vararg-
   collected массивы всегда свежие, без внешнего алиаса, см. находка §5) и
   bare-ro локалами (`!ctx.local_mut[n]`). `check_readonly_coerce_args`
   получил тот же scalar-exemption гейт. Закрывает H/I.
3. **Возврат (3-я позиция, Ф.1б).** Новая тройка `check_ro_launder_return`/
   `_in_block`/`_in_expr` (зеркалит существующий `check_closure_scalar_return*`
   traversal-паттерн) — хукнута в `f1_check_fn`'s `match &fd.body` рядом с
   `check_closure_scalar_return*`. Пропускает, если `ret.is_readonly()`
   (`-> ro T` уже фризит); иначе флагует bare `Ident` в `ro_binding_names`
   (минус scalar exemption). Покрывает trailing tail + explicit `return X`
   (включая вложенные if/match/loop-ветки).
4. **Scalar-primitive exemption (ФИНАЛ, владелец 2026-07-23).**
   `is_bare_scalar_primitive`/`is_bare_scalar_primitive_name` (types/mod.rs
   ~3074) — гейтит ВСЕ три позиции. Явно НЕ включает `str` (2 поля: ptr+len)
   и НЕ включает value-record (даже без кучевых полей, проба E).
5. **Overload-conflation fix (побочная находка, блокирующая).**
   `fn_mut_params`/`method_mut_params` ключуются по ИМЕНИ — для D326
   mode-overload (`fn f(x T)`/`fn f(mut x T)`/`fn f(consume x T)`) это
   конфлирует ВСЕ формы под одним ключом. Без фикса ломало зелёный
   `d326_mode_overload_axis.nv`. Исправлено: `ConsumeRegistry.fn_overload_names`/
   `method_overload_names` (count≥2) гейтят оба call-сайта
   `check_readonly_coerce_args`.
6. **Variadic-param exemption (побочная находка при внедрении Ф.1б).**
   `...args []T` — каждый call-site материализует СВЕЖИЙ массив, алиасить
   нечего. Без исключения ломало `Vec[T].of(...args) => args`. Гейтится в
   ro_binding_names-регистрации параметров (`!p.is_variadic`).

## §4. Ф.2 — миграция (полный список правок)

### std/src (13 сайтов, все закрыты)

- `collections/priority_queue_test.nv` — убран промежуточный re-init, `pq` mut с начала.
- `collections/range/core_test.nv` — аналогично, `it` mut с начала.
- `collections/vec_iter/core.nv` — `@fold[Acc](mut init Acc, ...)`, `@sum(mut zero T)`.
- `collections/vec_lazy/core.nv` — `@fold[Acc](mut init Acc, ...)`, `@sum(mut zero T)`,
  `box_iter` → `.clone()` (VecIter — новый `@clone()` метод в `vec/iter.nv`, см. ниже).
- `collections/vec/iter.nv` — добавлен `VecIter[T] @clone()` (shallow copy —
  независимый курсор `idx` над тем же buffer).
- `collections/vec_seq/core.nv` — `@fold[Acc](mut init Acc, ...)`.
- `crypto/sha256_test.nv` — убран промежуточный re-init.
- `testing/property.nv` — `shrink_loop(mut initial T, mut initial_failure PropertyFailed, ...)`
  + вызывающий сайт (`property_with`) адаптирован (`mut value`, `Fail(mut e)`).
- `time/cron.nv` — `dedup_sort`: `.clone()` (read-only-view contract) в ДВУХ
  местах (основная копия + early-return ветка — return-position находка).
- `unicode/collate.nv` — `push_one_ce`: `mut ce str` (in-out; `str` буфер
  always-ro anyway, D26) + вызывающий `push_ce_list`'s `for mut ce`.

### examples — 0 продакшн-хитов (только транзитивный nova-http через aggregator, вне мандата).

### spec_tests/conformance (20 файлов — 6 исходных + 14 из return-position волны)

Исходные 6 (до Ф.1б): `blanket_fold.nv`, `d228_value_record_copy_contract.nv`,
`m_embvt_protocolbox_embed_callarg_ok.nv`, `value_semantics.nv`,
`neg/mut_param.nv`, `standalone/f2_protocol_dispatch_method_survives.nv`.

Ещё 2 (найдены после отката временной scalar-exemption для верификации фикстур):
`d246_param_ro_mut_view.nv` (`xs` ro→mut, split-param combo test),
`d374_write_sink_decouple.nv` (`fmt_ctx` ro→mut — genuine H-form catch, sink
ДОЛЖЕН быть mut, это и есть его смысл).

Return-position волна (12 файлов, generic identity-passthrough `fn[T](x T) -> T => x`
паттерн — bound `SignedInts`/`Hash` НЕ распознаётся чекером как proof scalar-safety,
т.к. bound-aware анализа нет; везде фикс = `mut` на параметре, callers все
передают rvalue/литералы — 0 cascade):
`composition.nv`, `d16_generics_brackets.nv`, `d307_5_3_pf_dispatch_generic.nv`,
`d88_default_generic_params.nv`, `m196_facetc_instance_collision_and_method_generic_default.nv`,
`m196_mono_block_notrailing_return.nv` (2 функции, 3 параметра),
`m196af_freefn_arity_default_ret.nv`, `m196af_freefn_arity_default_ret_neg.nv`,
`p172_3_typeset_parse_smoke_positive.nv`, `stdlib_use.nv`,
`t3_pos_two_char_generic_ok.nv`, `turbofish_still_works.nv`,
`standalone/f3_generic_mono_on_use.nv`, `standalone/f3_generic_transitive_from_main.nv`.

4 файла в `neg/` (`multiple_sets.nv`, `not_in_set.nv`, `stdlib_neg.nv`,
`view_err_return.nv`) ТАКЖЕ получают новый `E_READONLY_COERCE` alongside их
СОБСТВЕННЫЙ ожидаемый код (`E_MULTIPLE_TYPE_SETS`/`E_TYPE_NOT_IN_SET`/
`D157-view-escape-return`) — проверено: их родной маркер по-прежнему
присутствует в выводе → тест не ломается (EXPECT_COMPILE_ERROR ищет
substring, не единственность) — оставлены БЕЗ правок.

### nova-http (соседняя репа) — НЕ мигрировано, только инвентарь

345 хитов на момент первого замера (`server/server_router.nv`,
`servernet/policy.nv` и др. — увидено транзитивно через `examples/flagship/aggregator`).
Вне моего мандата (чужой репозиторий) — список для интегратора/мейнтейнера
nova-http.

## §5. История находки (HARD-STOP → разрешение)

Изначальный порядок задания требовал снять инвентарь ДО фиксации нормы; норма
была уже зафиксирована владельцем до начала волны (коммиты `96557c61b`/
`c1904a989`). Реализация Ф.1 БЕЗ исключений вскрыла реальный объём:
`std/src` 721 хитов/165 файлов (baseline 27), `examples` 197/47, `nova-http`
345/41 — суммарно 1263, на порядок больше предполагавшихся 246. Абсолютное
большинство в `std/src` — безобидные копии СКАЛЯРНЫХ примитивов
(`fn f(n int) { mut end = n; ... }` — loop-counter/binary-search-midpoint
идиома). Провизорная scalar-exemption измерена (721→13 в `std/src`) и
доложена координатору вместе с полными числами (HARD-STOP-протокол задания).

**Разрешение (координатор, тем же сеансом):** scalar-primitive exemption
ВОЗВРАЩЕНА как часть ФИНАЛЬНОЙ нормы (не «временная уступка» — явное решение
владельца, подтверждённое дважды), с явной границей «скаляр-примитив ≠
value-запись» (проба E остаётся neg). Диагностика получила канон-порядок
подсказок (mut-параметр первым). Ф.2 продолжена под финальной нормой:
`std/src` 721→0, `examples` (собственный код) →0. Return-position (Ф.1б)
реализована в той же волне; при её внедрении вскрылась ВТОРАЯ волна
находок — bare generic-identity функции (`fn[T](x T) -> T => x`) в
spec_tests, не распознаваемые чекером как scalar-safe даже при
scalar-ограничивающем bound'е (`SignedInts`) — 30 хитов в spec_tests,
мигрированы тем же `mut`-параметр-паттерном (§4).

**Открытый follow-up (НЕ решаю сам — предлагаю, не блокирует):** bound-aware
анализ (`fn[T SignedInts] ...` — bound доказуемо scalar-only) мог бы убрать
необходимость ручного `mut` на identity-подобных generic-функциях; не
реализовано в этой волне (эффорт/время) — см. `[M-ro-launder-return-position-unimplemented]`→
теперь переименован в follow-up ниже.

## §6. Гейты — дословные вердикты

**`nova check std/src`** (финал, после миграции):
```
PASS: 142  FAIL: 27  WARN: 1040
```
Идентично baseline (`PASS: 142 FAIL: 27 WARN: 1040`, unmodified компилятор) —
0 `E_READONLY_COERCE`, все 27 FAIL — pre-existing (serde_neg/*, fs/neg/*,
io/neg/*, net/neg/*, time/civil/neg/*, testing/handlers/core.nv,
unicode/case.nv — все уже красные на baseline).

**`nova check examples`**:
```
PASS: 46  FAIL: 1  WARN: 377
```
1 FAIL = `examples/flagship/aggregator/src/main.nv` — git-fetch network
flake (nova-compress/nova-tls зависимости недоступны в sandbox), НЕ
E_READONLY_COERCE, воспроизводится на baseline тоже (сетевая среда).

**`nova check spec_tests`** (standalone per-file — НЕ мега-CU, baseline
всегда «красный» вне мега-CU по конвенции проекта):
```
PASS: 262  FAIL: 392  WARN: 1979 (после миграции, 0 неожиданных E_READONLY_COERCE —
14 hits остаются, ВСЕ из них — мои собственные m_ro_launder_* neg-фикстуры
(10) + 4 pre-existing neg-файла с alongside-совпадением, см. §4)
```

**`nova test` (полный pipeline, батчами, env NOVA_GC_LIB_DIR/NOVA_INCLUDE_DIR/
NOVA_GC_INCLUDE_DIR → main-repo vcpkg, libuv скопирован из main-repo без .git):**
- `std/src/collections` — `PASS: 13 FAIL: 0 SKIP: 7` (все touched-Ф.2-файлы внутри).
- `std/src/data std/src/encoding std/src/math` — `PASS: 15 FAIL: 0 SKIP: 28`.
- `std/src/fs` — `PASS: 1 FAIL: 0 SKIP: 3`; `std/src/io` — `PASS: 1 FAIL: 0 SKIP: 2`;
  `std/src/os` — `PASS: 1 FAIL: 0`; `std/src/prelude` — `PASS: 1 FAIL: 0 SKIP: 9`;
  `std/src/text` — `PASS: 3 FAIL: 0 SKIP: 3`; `std/src/checksums` — `PASS: 3 FAIL: 0 SKIP: 3`;
  `std/src/ffi` — `PASS: 0 FAIL: 0 SKIP: 1`; `std/src/time` — `PASS: 6 FAIL: 0 SKIP: 8`.
- `std/src/crypto/sha256_test.nv` (touched-файл, изолированно) — `PASS (2.85s)`.
- `std/src/testing/property_test.nv`, `std/src/crypto` (batch), `std/src/net`,
  `std/src/identifiers` — блокированы **pre-existing ICE** `[P67-LEGACY] Path
  call return type unknown for method=now` (`emit_c.rs:56222`/`:56079`) —
  **воспроизведено байт-в-байт на baseline `nova.exe` из главной репы** (не
  моя регрессия; известный класс `[M-fn-newtype-return-position-broken]`,
  см. `docs/plans/221.1-bug-sweep.md` №53). `unicode/collate.nv` не имеет
  выделенного test-файла в репо (pre-existing gap, не введён этой волной) —
  верифицирован только через `nova check` (типы чисты).
- `std/src/unicode`, `std/src/concurrency`, `std/src/_experimental` — не
  прогонялись целиком (время); `nova check std/src` (полный, покрывает ВСЕ
  директории типо-уровнем) уже подтвердил 0 регрессий по всему дереву.

**Мега-CU conformance + флагман-examples --strict-effects** — гейт
ИНТЕГРАТОРА (не выполнялся в этой волне намеренно — вне мандата
исполнителя по операционным правилам задания).

## §7. Фикстуры (все верифицированы дословно)

10 новых `spec_tests/conformance/{,neg/}m_ro_launder_*` — коды подтверждены
командой одним прогоном:
```
m_ro_launder_a_param_reinit_neg -> [E_READONLY_COERCE]
m_ro_launder_b_direct_index_write_neg -> [E_READONLY_CONTENT]   (регресс-пин B, без изменений)
m_ro_launder_d_local_reinit_neg -> [E_READONLY_COERCE]
m_ro_launder_e_pure_value_record_neg -> [E_READONLY_COERCE]     (проба E — value-record, НЕ scalar — остаётся neg)
m_ro_launder_f_vec_push_neg -> [E_READONLY_COERCE]
m_ro_launder_g_value_record_heap_field_neg -> [E_READONLY_COERCE]
m_ro_launder_h_arg_position_neg -> [E_READONLY_COERCE]
m_ro_launder_i_local_arg_position_neg -> [E_READONLY_COERCE]
m_ro_launder_j_l2_return_coerce_neg -> [E_READONLY_COERCE]      (регресс-пин J, без изменений)
m_ro_launder_return_position_neg -> [E_READONLY_COERCE]         (НОВАЯ, 3-я позиция — 2 hits, explicit return + trailing)
```
`m_ro_launder_scalar_primitive_pos.nv` — НОВЫЙ pos-фикстур (3 теста: local
re-init, param re-init, argument-position — все скалярные `int`) —
`PASS: 1 FAIL: 0` — подтверждает финальный exemption работает во ВСЕХ трёх
позициях.

## §8. Followups (обновлено)

- `[M-ro-launder-via-mut-binding]` — CLOSED в части нормы+чекера+миграции
  std/examples/spec_tests; nova-http остаётся как отдельная задача
  (не мой мандат).
- `[M-ro-launder-bound-aware-scalar-analysis]` (НОВЫЙ, P3, опционально) —
  чекер не распознаёт `fn[T SignedInts] f(x T)` как scalar-safe по bound'у;
  сейчас требует ручного `mut` на identity-подобных generic-функциях
  (мигрировано вручную, §4). Bound-aware анализ убрал бы эту ручную работу,
  но не реализован (эффорт/время этой волны).
- `[M-mut-params-registry-overload-conflation]` — `fn_mut_params`/
  `method_mut_params` конфлируют перегрузки по имени; пофикшено ТОЛЬКО для
  этого чекера (`fn_overload_names`/`method_overload_names` guard) — тот же
  корень может задевать `check_unsafe_coerce_args` (аналогичная структура,
  НЕ проверено в этой волне).
- `[M-ro-launder-nova-http-migration]` (НОВЫЙ) — 345 хитов в nova-http/src
  (соседняя репа), инвентарь снят (`server/server_router.nv:308,321`,
  `servernet/policy.nv:97` и др. — полный список не приложен, снимается
  через `nova check` из nova-http worktree с бинарём этой волны), волна
  миграции — задача мейнтейнера nova-http.
- Pre-existing (НЕ вводится этой волной, только обнаружено при верификации):
  `[M-fn-newtype-return-position-broken]` ICE (`P67-LEGACY`, `method=now`)
  блокирует `nova test` для `net`/`identifiers`/части `crypto`/`testing` —
  уже задокументировано в `docs/plans/221.1-bug-sweep.md` №53, вне scope
  этой волны.
