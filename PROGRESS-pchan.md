# PROGRESS — окно p-chan (Channel[T] настоящая мономорфизация)

Состав: №286 + №143 (+ снос `[M-59.1-channel-new-cleanup]`), остаток блока S1b.
Worktree: `d:/Sources/nv-lang/nova-chan`, ветка `pchan` от `main` (394870ce7).

## Диагноз, подтверждённый чтением кода (не только реестром)

`Channel`/`ChanReader`/`ChanWriter` — ПОЛНОСТЬЮ compiler-intrinsic: нет ни
одного `.nv`-объявления (`external type`/`external fn`) нигде в `std/` или
`spec_tests/`. Подтверждено:
- `compiler-codegen/src/lib.rs:59-68` (`is_intrinsic_namespace`) — просто
  список имён, не реестр сигнатур.
- Grep по `std/**/*.nv` на `fn ChanReader`/`fn ChanWriter`/`Channel[T].new` —
  0 реальных объявлений (`timer.nv`'s `ChanReader_close_after_doc_marker` —
  documentation-only заглушка, тело пустое).

Следствие: методы `.recv`/`.try_recv`/`.send`/`.try_send`/`.share`/... НЕ
зарегистрированы в `self.sig`/`method_overloads` — общий генерик-диспетчер
чекера `infer_method_call_channel_type` (types/mod.rs:20202) для них
структурно НЕ может сработать (там просто нет декларации, по которой искать).
Отсюда и диагноз №143/№286: `T` нигде не течёт дальше `Channel.new`, `recv()`
статически типизируется `Option[int]` ИМЕННО ПОТОМУ, что чекер вообще не
знает о существовании декларации с генериком `T` для этого метода.

Второй, независимый факт: `scope: HashMap<String, TypeRef>` (главный
чекер-проход, `f1_stmt`/`f1_expr_inner`, types/mod.rs) НИКОГДА не получает
запись для `tx`/`rx` из `let (tx, rx) = Channel.new(...)` — ни в tuple-, ни
в record-форме. `f1_stmt`'s `Pattern::Tuple`-ветка (~7841) типизирует
элементы ТОЛЬКО когда RHS даёт `TypeRef::Tuple` — а `Channel.new` таким RHS
никогда не является (не зарегистрированная fn). Единственное место, где
`tx`/`rx` получают тип `"ChanWriter"`/`"ChanReader"` (БЕЗ generic-параметра!)
— отдельный `ConsumeCtx` (~37375-37407), используемый ТОЛЬКО линейным
чекером consume/`send`-параметра (№144), не основным типовым проходом.

Но: параметры функций — `fn relay(rx ChanReader[int], tx ChanWriter[int])`
(форма УЖЕ документирована в channels.md и УЖЕ парсится) — получают
`scope[name] = p.ty.clone()` НАПРЯМУЮ (types/mod.rs:7543-7544) — то есть
`ChanReader[int]` КАК ТИП уже долетает до `scope` бесплатно, если тип явно
аннотирован. Дыра — только у `Channel.new`'s ВОЗВРАЩАЕМОГО значения.

## Точки врезки (план, реализован этой волной)

Все правки — **types/mod.rs** (чекер-канал, §0/196), НЕ emit_c.rs:

1. **`channel_new_turbofish_elem(value: &Expr) -> Option<TypeRef>`** — новый
   module-level helper рядом с `is_channel_new_call` (~37076): извлекает
   явный `T` из `Channel[T].new(...)` (receiver-turbofish, документированная
   форма) либо `Channel.new[T](...)` (method-turbofish, симметрии ради).
   Бесплатно (без extra work) на BARE `Channel.new(cap)` — `None`, T
   остаётся неотслеженным — **строго аддитивно**, ноль регрессии для
   существующего непомеченного кода.

2. **`f1_stmt`'s `Pattern::Tuple`-ветка** (~7841): новая ветка ПЕРЕД
   существующей generic tuple-логикой — `pats.len()==2 &&
   is_channel_new_call(&d.value)` → `scope["tx"] = ChanWriter[T]`,
   `scope["rx"] = ChanReader[T]` (T из `channel_new_turbofish_elem`, либо
   `generics: vec![]`, если T неизвестен — та же гарантия аддитивности).
   Существующая generic-tuple-ветка становится `else`-веткой, byte-identical
   для всех НЕ-Channel.new двух-элементных tuple-let.

3. **`channel_elem_type(&self, obj: &Expr, scope) -> Option<TypeRef>`** —
   новый метод `TypeCheckCtx`: если `infer_expr_type(obj, scope)` резолвится
   в `TypeRef::Named{path, generics}` с `path.last() ==
   "ChanReader"|"ChanWriter"` И `generics.len()==1` — возвращает T. `None` —
   ЛЮБОЙ другой случай (untracked/erased channel) → вызывающий код обязан
   трактовать как «падаем в legacy», НИКОГДА не как ошибку.

4. **`infer_expr_type`'s `Call`-ветка** (~18424, сразу после существующего
   FixedArray `len`/`ptr`-спецкейса — тот же стиль): `.recv()`/`.try_recv()`
   на ресивере с известным T → `Option[T]` (`TypeRef::Named{path:["Option"],
   generics:[T]}` — тот же паттерн, что уже используют `closure_if_ctor_peek`
   (~20650) и Member `Some(x)`-конструктор-продюсер (~9284)). `.share()` на
   `ChanWriter[T]` → тот же `ChanWriter[T]` (тип ресивера).

5. **`f1_expr_inner`'s резолв-канал-продюсер** (~8867, `else if let
   ExprKind::Member { obj: mo, name: method } = &func.kind` — ветка, которая
   уже накрывает TurboFish-статик-ctor/`__array`/module-qualified static —
   добавлена СИММЕТРИЧНАЯ новая ветка ДЛЯ recv/try_recv/share): та же логика,
   что (4), но материализует в `resolved_types_buf` (`ResolvedType::from_
   type_ref`, `gs`-gate через `typeref_mentions_any`, byte-паттерн `Some(x)`-
   продюсера ~9284). Это и есть «Channel 2» — codegen's `infer_expr_c_type`
   читает `resolved_types` УЖЕ СЕЙЧАС (Channel 2, emit_c.rs:59712+) и уже
   умеет лоуэрить `Option[T]` для ЛЮБОГО T через общий `resolved_type_to_c`/
   `resolved_named_to_c`'s `"Option"`-ветку (emit_c.rs:4537-4596) — НИКАКИХ
   изменений в emit_c.rs для самого лоуэринга не понадобилось: инфраструктура
   генериков УЖЕ универсальна, дыра была ТОЛЬКО в чекере, не докладывавшем
   истину по каналу.

6. **`f1_expr_inner`'s `write`/`write_at`/... pointer-writability check**
   (~8725) — рядом добавлена симметричная проверка для
   `.send(v)`/`.try_send(v)`: когда `channel_elem_type(obj, scope)` знает T —
   `self.assignable(arg, T, ...)` (тот же helper, что общий
   arg-type-check у обычных вызовов, ~13451) → новая диагностика
   `[E_CHANNEL_ELEM_TYPE_MISMATCH]` на `Compat::Bad`/`Narrowing`/
   `OutOfRange`/`CoerceConflict`, `Compat::Ok|Unknown` — молча ОК (как
   везде).

## ЧТО НЕ СДЕЛАНО в этой волне (осознанно, с обоснованием)

**Снос 3 ad-hoc codegen-веток `Channel.new` + настоящий кортежный возврат
`(tx, rx)` — НЕ РЕАЛИЗОВАН.** Обоснование:

- Три ветки — это ТРИ AST-формы, которыми codegen узнаёт вызов
  `Channel.new`/`Channel[T].new` (bare Ident / TurboFish-ресивер / Path),
  зеркало `is_channel_new_call`'s тройного match'а — они эмитят
  `Nova_ChannelPair` (ad-hoc C-struct, `nova_rt/channels.h`) вместо
  РЕАЛЬНОГО generic-tuple-возврата через уже landed Plan 59.1 инфраструктуру
  (generic anonymous tuple mono, ЗАКРЫТА 2026-06-01 для произвольных
  user-fn, Ф.3 «Channel.new ad-hoc cleanup» — deferred).
- Исходный маркер `[M-59.1-channel-new-cleanup]` (Plan 59.1, docs) требовал
  «Nova-side `external fn[T] Channel[T].new` через Plan 115 Pattern B».
  **Проверено этой волной: Plan 115 (`docs/plans/115-ptr-type-and-tuple-ffi.md`)
  до сих пор `🆕 PLANNED` — НЕ реализован** (оценка автора плана — ~1-1.5
  dev-day САМ ПО СЕБЕ, отдельный P1-план: `ptr`-тип, tuple-by-value FFI ABI,
  opaque handle pattern, свой D-блок D214). Строить снос ad-hoc-веток на
  фундаменте, которого нет, в это окно — нереалистично.
- **Уточнение находки этой волны:** `Channel.new` зовёт СОБСТВЕННЫЙ
  runtime-intrinsic (`nova_channel_new`), не настоящую внешнюю C-библиотеку
  — то есть Plan 115's FFI-ABI слой (пересечение чужой ABI-границы) строго
  не требуется; технически снос МОЖНО было бы сделать, зарегистрировав
  `Channel.new` как обычную generic-fn-декларацию в `self.sig`/
  `method_overloads` (тело — intrinsic-байпас в codegen, как уже делают
  `size_of[T]()`/effect-опы), и подключив её RETURN к уже работающей
  tuple-mono инфраструктуре Plan 59.1. Это меньше, чем полный Plan 115, но
  всё ещё: (а) риск для ВСЕХ существующих callsites `Channel.new` по всему
  репо/пользовательскому коду (std, тесты, примеры, флагман — самый
  часто используемый конкурентный примитив проекта); (б) требует точного
  понимания naming/материализации Plan 59.1's tuple-mono схемы (`NovaTup2_
  X_Y`-подобной), не тронутого сегодня; (в) НЕ требуется для закрытия
  измеренного P0/P1-дефекта (№286 — тихая мискомпиляция) — тот полностью
  устраняется чекер-фиксом выше, НЕЗАВИСИМО от внутреннего представления
  `Channel.new`'s возврата (`Nova_ChannelPair`-struct vs настоящий tuple).
- Решение: не трогать в этом окне — риск/объём непропорционален выигрышу
  сверх уже закрытой типовой дыры. Заведён follow-up-маркер (см. ниже),
  честно со ссылкой на Plan 115 gap и на этот design-note.

**Слот-ограждение `E_CHANNEL_UNSOUND_ELEM_TYPE`** — слот остаётся
`nova_int`-sized (не по размеру `T`) — брифом это explicitly НЕ обязательно.
Сообщение диагностики ПЕРЕСМОТРЕНО (см. Фаза 3 ниже): теперь ссылается на
объявленный `T` канала, а не только на «payload C type».

**Pattern::Record-форма** (`ro { tx, rx } = Channel.new(...)` /
`{ tx: sender, rx: receiver }`) и record-ACCESS форма (`ch.tx`/`ch.rx`) —
НЕ получили нового T-tracking этой волной: в главном чекер-проходе
(`f1_stmt`) вообще нет существующей ветки для типизации `Pattern::Record`
destructure (более широкий, отдельный, pre-existing пробел, не специфичный
для Channel) — расширять его контрабандой в это окно означало бы
незапланированный рост объёма. Ни один обязательный фикстур-сценарий этой
волны не использует эти формы (все — `ro (tx, rx) = Channel[T].new(...)`,
что совпадает с существующим стилем во ВСЕХ текущих Channel-фикстурах
репозитория). Отмечено как остаток, не блокирует №286/№143.

## Статус фаз

- Фаза 0 — design note (этот файл). ✅ ГОТОВО.
- Фаза 1 — чекер-канал (правки types/mod.rs, пп. 1-6 выше, плюс
  Pattern::Record-ветка для `{tx,rx}`/`{tx:sender,rx:receiver}`
  destructure — добавлена по факту (существующие фикстуры используют её
  чаще tuple-формы), задокументирована рядом с tuple-веткой). ✅ ГОТОВО,
  `cargo build --release` чист (nova-cli + nova-codegen).
- **Фаза 1b (НЕПЛАНИРОВАННАЯ, найдена тестированием) — emit-сторона
  `recv`/`try_recv`.** Чекер-фикс сам по себе оказался НЕДОСТАТОЧЕН:
  рантайм-функция `nova_chan_reader_recv`/`_try_recv` физически всегда
  возвращает erased `NovaOpt_nova_int` (одна word-sized ячейка буфера) —
  как только чекер стал честно обещать `Option[T]` для T≠int, emit
  перестал совпадать с обещанным C-типом → CC-FAIL. Измерено на
  СУЩЕСТВУЮЩЕЙ фикстуре `channel_elem_type_word_safe.nv`'s
  `Channel[bool].new`-турбофиш тесте (регрессия, не гипотеза). Фикс:
  `channel_recv_target_c`/`channel_reinterpret_novaopt` (emit_c.rs) —
  реинтерпретирует сырое `NovaOpt_nova_int` в `NovaOpt_<T>`, с учётом ДВУХ
  разных раскладок NovaOpt (`register_novaopt_decl`'s NPO для
  pointer-`T` — `{value}` БЕЗ `.tag`; tagged `{tag,value}` для scalar-`T`
  — измерено, оба варианта ловили РАЗНЫЕ CC-FAIL до этого различения).
  ✅ ГОТОВО, arch-ratchet lines 64396→64505 осознанно поднят с построчным
  обоснованием (`scripts/guards/arch-ratchet.baseline`), infer=348 не
  сдвинут.
- Фаза 2 — фикстуры: 3 neg (`channel_elem_type_mismatch_str_neg.nv`,
  `channel_elem_type_mismatch_same_size_newtype_neg.nv`,
  `channel_recv_wrong_var_type_neg.nv`) + 1 pos
  (`pos_channel_elem_type_record_sum_map.nv`: record/sum/HashMap) +
  существующая `pos_channel_send_consume_share.nv` (Vec[int], регресс
  s1b-находки). ✅ ГОТОВО, `nova lint` — 4 файла, 0 находок.
  **Стандалон-прогоны индивидуальных neg-фикстур зелёные** (см. отчёт).
  Комбинированный/мега-CU прогон — см. Фазу 5 ниже (в процессе на момент
  записи).
- Фаза 3 — codegen: сообщение `E_CHANNEL_UNSOUND_ELEM_TYPE` пересмотрено
  (оба сайта, `send`/`try_send`) — теперь ссылается на «объявленный T
  канала» и на новый `[E_CHANNEL_ELEM_TYPE_MISMATCH]` как основную
  типовую проверку; этот гейт остался как ортогональное
  runtime-representation ограничение. ✅ ГОТОВО.
- Фаза 4 — доки: `channels.md`/`channels.ru.md` — секция «T inferred from
  first send/recv» переписана (T отслеживается ТОЛЬКО при явном
  турбофише/аннотации; голый `Channel.new` — по-прежнему erased,
  permissive). Спек-амендмент `D91` (2026-08-02) в
  `spec/decisions/06-concurrency.md` — читает как продолжение
  существующего амендмента 2026-07-27 (№144). ✅ ГОТОВО.
- Фаза 5 — гейты: `cargo build --release` чист; `nova lint` 0 находок;
  arch-ratchet OK (path B, обоснован); `nova check std/src` и
  комбинированный/парный прогон фикстур — см. финальный отчёт для точных
  чисел (гейты запущены, ждут завершения на момент последней правки этого
  файла).

## Дефекты, найденные И ПОЧИНЕННЫЕ в этом же окне (не остаются открытыми)

- **Emit-сторона `recv`/`try_recv` не совпадала с новым честным
  checker-типом** (см. Фаза 1b) — почина ТОЙ ЖЕ волной, не заведён
  отдельным номером в реестре (zero-tolerance: дефект своей же волны
  чинится в ней же, не маркером).
