# Окно p383-bounds — №383 + №384

Ветка `p383-bounds`, worktree `d:/Sources/nv-lang/nova-p383`. Модель: sonnet.
Материал: `docs/plans/wip/PROGRESS-audit-enforcement.md` (worktree `nova-audit`,
ветка `p-audit-enforcement`), пробы `probes/p1..p17` того же worktree,
реестр `| 383 |` / `| 384 |`.

## Единый вход — где он был и как обе формы к нему сведены

**До фикса** бáунд-проверка call-site была буквально ДВУМЯ независимыми
реализациями в `compiler-codegen/src/types/mod.rs`:

- `check_call_bounds` (свободная функция) — turbofish ИЛИ arg-based
  inference (`param_generic_name` + `infer_arg_ty` по позиционным `args`)
  для КАЖДОГО generic-параметра callee.
- `check_method_call_bounds` (метод) — делал СОВЕРШЕННО ДРУГОЕ: только
  receiver-substitution ПЕРВОГО generic'а (Plan 101 формы `fn[T Bound] []T
  @method` / `fn[T Bound] T @method`), причём даже не получал `args` с
  call-site — а для любого другого (например номинального `Box13`)
  receiver-типа просто `return`-ил без единой проверки. Method-own
  generics (`fn Recv @method[S Bound](args)`) не проверялись НИКОГДА — не
  «расхождение check/build», а полное отсутствие механизма.

Это и есть третья «вторая дверь» дня — по факту `if is_method {...} else
{...}`, просто разнесённый по двум разным функциям вместо явного if.

**После фикса** — один вход, `check_generic_bounds_for_call(callee,
callee_name, type_args, args, recv_binding, span, scope, errors)`:
источники биндинга по приоритету — (1) `recv_binding` (опциональный,
заполняется ТОЛЬКО когда receiver сам является Plan-101 typevar'ом), (2)
turbofish, (3) arg-based inference по `callee.params`/`args` — ОДИН и тот
же цикл для метода и свободной функции. `check_call_bounds` зовёт его с
`recv_binding: None`; `check_method_call_bounds` — с `recv_binding:
Some((name, ty))` ТОЛЬКО когда receiver-key резолвится в Plan-101
сентинел (`"[]T"`/`"T"`), иначе `None` — и тогда все generics callee,
включая method-own, идут через тот же arg-based путь, что у свободной
функции. Ветка ровно одна: `if is_method` нигде не появляется — разница
между формами свелась к ОДНОМУ опциональному параметру.

Заодно исправлен смежный баг в receiver-key resolution: старый код у
`check_method_call_bounds` при Array-receiver'е (`v: []u8`) всегда
пробовал ТОЛЬКО ключ `"[]T"` (Plan-101 сентинел) — но D239-канонизация
(`[]T ≡ Vec[T]`), которую использует ВЕСЬ остальной checker (see
`check_instance_overload` ~13880/~20872), кладёт carrier-form методы
(`Vec[T] mut @append[S AsSlice[T]]`) под ключ `"Vec"`, а concrete-facade
методы (`fn []u8 @to_str()`) — под `"[]<elem>"`. Три РАЗНЫХ ключа под
одной синтаксической формой. Новый код пробует все три по приоритету
(`"Vec"` → `"[]<elem>"` → `"[]T"`), как это уже делает
`check_instance_overload` в соседнем месте — не новая логика, воспроизведён
существующий D239-паттерн.

Файл: `compiler-codegen/src/types/mod.rs`. Ключевые функции:
`check_call_bounds` (~26356), `check_generic_bounds_for_call` (новая,
~26397), `check_method_call_bounds` (~26520).

## №384 — тот же принцип единого входа

**Корень:** `check_satisfaction_against_methods` (используется ОБОИМИ путями
выше — и call-site свободной функции, и call-site метода, через
`check_satisfaction`) безусловно принимал ЛЮБОЙ required-метод с
`default_body` как «удовлетворён» (`if req.default_body.is_some() {
continue; }`), не проверяя, что ЗАВИСИМОСТИ этого default-тела
(`Equal.equal => @compare(other) == 0` требует `@compare`) реально
резолвятся для конкретного типа. При этом РОВНО ТАКАЯ ЖЕ проверка уже
существовала — `default_body_calls_satisfy_for` — но использовалась
ТОЛЬКО на decl-site (`TypeCheckCtx::verify_impl_protocols`, проверка
`#impl(P)`), а не на call-site (`BoundCtx::check_satisfaction_against_methods`).
Классическая вторая дверь: одна и та же дыра, один и тот же уже готовый
чинящий механизм, просто не подключённый ко второму входу.

**Фикс — реюз, не дублирование.** `default_body_calls_satisfy_for` +
3 walker-функции (`walk_default_body_block/stmt/expr`) были МЕТОДАМИ
`TypeCheckCtx` (используют `self.t_provides_method`/`t_provides_field`/
`t_satisfies_str_from`, которых у `BoundCtx` нет — разные структуры,
разные наборы данных: `TypeCheckCtx` дополнительно несёт synth/auto-derive
overlay и per-type field table). Извлёк их в СВОБОДНЫЕ функции,
параметризованные трейтом `DefaultBodyProbe` (`provides_method`/
`provides_field`/`satisfies_str_from`) — сам обход (walker) теперь ОДИН,
каждый контекст поставляет свой backend:

- `impl DefaultBodyProbe for TypeCheckCtx` — как раньше, полный (synth
  overlay + field table).
- `impl DefaultBodyProbe for BoundCtx` — только базовый `sig.method_table`
  (`provides_method`), `provides_field` консервативно `false` (у BoundCtx
  нет per-type field table; сегодня единственный default-body протокол,
  `Equal`, зависит только от МЕТОДА `@compare`, не от поля — эта ветка
  сегодня не задействуется), `satisfies_str_from` — та же логика через
  `sig.methods_of`. Это СТРОГОЕ ПОДМНОЖЕСТВО того, что умеет
  `TypeCheckCtx`-бэкенд — может сделать проверку ТОЛЬКО консервативнее
  (иногда лишний "missing" там, где decl-site знает про synth), никогда
  не слабее — риска замаскировать реальную дыру нет.

`TypeCheckCtx::default_body_calls_satisfy_for` остался тонким
делегатором (`default_body_calls_satisfy_for(body, tname, self)`) — все
существующие вызовы (decl-site, ещё один call-site helper ~16675) не
тронуты. `check_satisfaction_against_methods` теперь зовёт свободную
функцию напрямую: `default_body_calls_satisfy_for(body, &concrete_name,
self)`.

Файл: `compiler-codegen/src/types/mod.rs`. Trait + free functions —
~25139 (между `impl TypeCheckCtx` и `struct BoundCtx`); `impl
DefaultBodyProbe for BoundCtx` — сразу после `struct BoundCtx`
(~25385); фикс в `check_satisfaction_against_methods` — ~28115.

## Все места механизма → вердикт

| механизм / форма | где | вердикт ДО | вердикт ПОСЛЕ |
|---|---|---|---|
| Метод-own bound, номинальный receiver (`fn Box13 @combine[S Clone]`) | `docs/plans/repro/p383/p383_method_bound_neg` | ok/built (ложный, тихий identity) | **FAIL** (корректно) |
| То же, аргумент УДОВЛЕТВОРЯЕТ бáунд | `docs/plans/repro/p383/p383_method_bound_pos` | — (не тестировалось) | **ok** (корректно, регресс не внесён) |
| Метод-own bound, carrier-receiver (`Vec[T] @append[S AsSlice[T]]`, №381) | `docs/plans/repro/p383/p383_asslice_carrier_neg` | ok (ложный), build падает на C (расхождение) | **FAIL** (корректно, с явным списком отсутствующих методов) |
| Свободная функция `fn[T Bound] freeFunc` (Hash/Clone/Write/Next/Iter/Index/MutIndex) | пробы `p2,p3,p6,p7,p9,p10,p11,p12` (worktree nova-audit) | FAIL (корректно) | **FAIL** (без изменений — регресс не внесён) |
| Plan 101 receiver-prefix (`fn[T Bound] []T @method`) | не отдельно пробовано — тот же `recv_binding`-путь, покрыт существующими vec.nv-методами в std-корпусе | работало | работает (std-корпус 148/26/61 без изменений) |
| Receiver-carrier bound (№303, `fn Vec[T Printable] @method`) | смежный, отдельный механизм `check_receiver_carrier_bounds` | уже фикшено раньше | не тронуто этим окном |
| `Equal` call-site (`fn[T Equal] use_equal(a,b) => a.equal(b)`) | `docs/plans/repro/p383/p384_equal_callsite_neg` | ok/built (UB — `Nova_HashMap_method_equal(a,(void*)(b))`, verified в сгенерированном C) | **FAIL** (корректно: «`NoEqual8` is missing: equal(...)») |
| `#impl(Equal)` decl-site, поле без Equal-зависимостей (int) | `docs/plans/repro/p383/p384_equal_implsite_pos` | ok/built, рантайм печатает корректное structural-сравнение | **ok** (НЕ БАГ — см. ниже; оставлено как pos, не тронуто) |
| `#impl(Equal)` decl-site, поле БЕЗ auto-derive (fn-типа) | `docs/plans/repro/p383/p384_equal_implsite_neg` | не пробовалось | **FAIL** (E_AUTO_DERIVE_FIELD_LACKS_PROTOCOL + E_IMPL_MISSING_METHODS — decl-site реально энфорсит, когда синтез невозможен) |
| `str` → `[]u8` через `#coerce`, бáунд `AsSlice` | `docs/plans/repro/p383/p383_coerce_asslice_pos` | — | **ok**, build+run корректны (см. вопрос про #coerce ниже) |
| Тип-уровневый bound (`type IndexMap[K Equal + Hash, V]`) | `docs/plans/repro/p383/p384_typebound_gap` | не проверялось | **ok** (НЕ ИСПРАВЛЕНО — №TBD, см. ниже) |
| `E_POINTER_RO_MUT_METHOD` (mut-метод через `*ro T`) | `docs/plans/repro/p383/p_ro_mut_method_gap` | по аудиту 231.1 — emission НЕТ | **ok** (НЕ ИСПРАВЛЕНО, подтверждено живо — №TBD, см. ниже) |

### std-корпус ≥9 сигнатур method-own bound (реестр аудита)

Все проверены грепом на предмет что канон `nova check std/src` (148/26/61)
не сдвинулся:

- `vec/access.nv:220` `@binary_search_by_key[K Compare]` — не задет (std
  сам не нарушает).
- `vec/mutate.nv:262` `@extend[S Iter[T]]` — не задет.
- `vec/mutate.nv:283` `@append[S AsSlice[T]]` (№381) — не задет (std не
  вызывает `@append` с несовместимым типом нигде).
- `vec/sort.nv:232,242` `@sort_by_key`/`@sort_unstable_by_key[K Compare]`
  — не задет.
- `vec/sort.nv:287` `@dedup_by_key[K Equal]` — не задет.
- `encoding/serde/serde.nv:186,202,206,210,214,218,285,319,340`
  `@serialize[S Serializer]` (×9) — не задет.

Канон держится: **148 PASS / 26 FAIL / 61 WARN**, ровно те же 26 файлов
(все — намеренные `*_neg/`-фикстуры serde/fs/io/net/time, не новые
находки).

## Ответ на вопрос владельца: удовлетворяет ли `#coerce` бáунд протокола?

**Факт, эмпирически проверено (`docs/plans/repro/p383/p383_coerce_asslice_pos`,
build+run):** `[]u8.new().append(some_str)` компилируется И работает
корректно (напечатанная длина совпадает с длиной строки).

**Но НЕ потому, что бáунд-чекер понимает `#coerce`.** Механизм —
совпадение двух независимых фактов:

1. `check_satisfaction_against_methods` (types/mod.rs ~28039) содержит
   ЖЁСТКИЙ ранний `return` для фиксированного списка примитивных имён
   (`int`/`i8`/…/`str`/`any`/`never`) — ЛЮБОЙ бáунд считается
   удовлетворённым для этих имён БЕЗ структурной проверки, независимо от
   того, реально ли примитив предоставляет требуемые методы.
2. Codegen ОТДЕЛЬНО и корректно применяет `#coerce fn str @bytes() -> ro
   []u8` (`std/src/runtime/string/core.nv:74`) в позиции call-arg — то
   есть в `@append` реально попадает уже сконвертированный `[]u8`,
   который структурно удовлетворяет `AsSlice`.

Эти два механизма НЕЗАВИСИМЫ и просто совпали для `str`. `str` НЕ несёт
`#impl(AsSlice[u8])` и не имеет `@len` (только `@byte_len()`) — то есть
структурно `str` не удовлетворяет `AsSlice` вообще; бáунд-чекер это не
видит только потому, что `str` попадает под примитивный allowlist раньше,
чем до структурной проверки доходит очередь.

**Практический вывод:** пользовательский тип со своим `#coerce` к
AsSlice-совместимому типу НЕ получит такой же бесплатный пропуск — только
буквальные примитивы из хардкод-списка. Бáунд-чекер сегодня НЕ умеет
консультироваться с `#coerce`-конверсиями в принципе (ни для примитивов
принципиально, ни для user-типов). Владельцева идиома `[]u8.new().append
(some_str)` УЖЕ работает — но по счастливой случайности примитивного
allowlist'а, не по продуманному дизайну. Если понадобится тот же паттерн
для НЕ-примитивного типа с `#coerce`, потребуется отдельный фикс —
научить `check_satisfaction`/`check_satisfaction_against_methods`
проверять `#coerce`-путь явно.

## №TBD находки (не чинились этим окном)

### №TBD-1 — бáунд на типовом параметре ТИПА (не метода, не функции) не проверяется вообще

По запросу параллельной сессии (координатор): `type IndexMap[K Equal +
Hash, V] priv { ... }` (`std/src/collections/index_map/core.nv:56`) —
бáунд `Equal + Hash` объявлен на generic-параметре САМОГО ТИПА. Это
ЧЕТВЁРТАЯ форма носителя бáунда (свободная функция / метод-own / Plan-101
receiver-prefix / receiver-carrier — все покрыты; тип-декларация — нет).
Грепом подтверждено: НИ ОДНОГО места в `types/mod.rs`, проверяющего
`TypeDecl.generics`-бáунды на instantiation-сайте. Проба
(`docs/plans/repro/p383/p384_typebound_gap`): `IndexMap[NoEqualHash, int].new()` с
типом БЕЗ `@equal`/`@hash` — `nova check` пропускает чисто.

`nova build` НЕ производит тихий UB так же прямолинейно, как №384 —
падает РАНЬШЕ, на несвязанной codegen-ошибке (naming mismatch в
монoморфизации итератора `Vec[Slot[K,V]]` внутри resize-пути HashMap:
`unknown type name 'NovaValue_VecIter____...'`), не доходя до точки, где
реально понадобились бы `@equal`/`@hash` у `NoEqualHash`. Смежная находка:
`std/src/collections/hash_map/core.nv:38-42` содержит УСТАРЕВШИЙ
комментарий — «`[K Hash + Equal]` будет формальным когда multi-bound
syntax появится в bootstrap parser» — multi-bound (`K A + B`) синтаксис
УЖЕ существует и используется (тот же `IndexMap`!), но `HashMap[K Hash,
V]` так и не был обновлён до `[K Hash + Equal]`; ключевое сравнение внутри
`HashMap` использует `==` (не `.equal()`), что для non-primitive K —
отдельный, не до конца прослеженный в этом окне путь (нужен фикс
несвязанной monomorphization-ошибки, чтобы дойти до наблюдения его
рантайм-поведения).

**Не чинилось**: полноценный фикс — новый, самостоятельный механизм
(проверка бáундов на TypeDecl.generics в КАЖДОМ instantiation-сайте типа —
`Type[Args].method()`, поле структуры, `let x Type[Args]`, параметр
функции), заметно больше по объёму чем reuse существующего пути; отдельное
окно.

### №TBD-2 — `E_POINTER_RO_MUT_METHOD` подтверждён живым и незакрытым

По запросу координатора: спека (02-types.md:9700/10497) обещает mut-метод
через `*ro T`(bare `*T`) обязан отлетать `E_POINTER_RO_MUT_METHOD`. Грепом
(до и после правок этого окна) — 0 упоминаний кода нигде в компиляторе.
Живая проба (`docs/plans/repro/p383/p_ro_mut_method_gap`):

```nova
type Counter { mut v int }
fn Counter mut @bump() -> @ { @v += 1 }
fn main() Io -> () {
    mut x = Counter { v: 0 }
    ro p *Counter = &x
    p.bump()
    print("${x.v}")   // печатает "1"
}
```

`nova check` пропускает чисто — БЕЗ диагностики вообще, даже
`E_UNSAFE_UNUSED` не срабатывает (auto-deref метод-вызов через
raw-pointer не распознаётся D216 §21-картой как unsafe-требующая
операция, значит и без `unsafe {}` обёртки компилируется). `nova build` +
запуск: мутация через объявленный `ro`-указатель РЕАЛЬНО происходит на
исходном биндинге — не просто диагностика отсутствует, реальный
soundness-баг (ro-указатель мутирует через него же).

Это ДРУГОЙ корень, чем сегодняшний №377 («мусор возвращается через
указатель» — deref-направление); здесь «разрешён ли ВЫЗОВ mut-метода через
ro-указатель» — не энфорсится нигде. Родственный работающий механизм —
`E_POINTER_RO_ASSIGN` (`types/mod.rs` ~9067-9089) — но он гейтит
ЖЁСТКО ЗАШИТЫЙ список raw-pointer intrinsic-имён
(`write`/`write_at`/`write_unaligned`/`write_volatile`/`copy_from`/
`copy_from_nonoverlapping`), НЕ произвольные user `mut @method`-вызовы.
Вероятная форма фикса — расширить тот же `if let ExprKind::Member{obj,
name}`-гейт: для ЛЮБОГО резолвленного метода с `Receiver.mutable ==
true`, если `pointee_is_writable(&obj_ty) == Some(false)` — та же
диагностика `E_POINTER_RO_MUT_METHOD` (не `E_POINTER_RO_ASSIGN`, свой
код).

**Не чинилось** — отдельный механизм (pointer safety, не generic-bound
checking), вне объёма №383/№384; координатор просил проверить/доложить,
не чинить.

## D-амендмент (текст для интегратора — спеку не правил)

**02-types.md, около §7185 (Plan 101.2 запись)** — уточнить формулировку:
текущая запись «receiver-generic `fn[T Bound] []T @m` ловит violation на
call-site» была ВЕРНА только для Plan-101 prefix-формы; после этого окна
(№383) call-site bound enforcement распространяется на METHOD-OWN
generic-параметры (`fn ReceiverType @method[S Bound](...)`, вне
зависимости от того, номинальный ли receiver, `[]T`-prefix или
carrier-form `Vec[T]`) — единым путём с `check_call_bounds`. Ранее
эти формы вообще не проверялись (не «расхождение», а отсутствие
механизма); теперь `NoBound.method(...)` для ЛЮБОЙ из этих форм ловится
на `nova check` с тем же диагностическим форматом «type `X` does not
satisfy `P` bound».

**02-types.md, §8300-8380 (`E_IMPL_MISSING_METHODS`, `#impl(P)`
verification)** — добавить: default-body-зависимость протокольного
метода (`Equal.equal => @compare(other) == 0`) теперь проверяется НЕ
только на decl-site (`#impl(P)`, было верно и до этого окна), но и на
CALL-SITE `[T P]` generic-бáунда (`fn[T Equal] f(a T, b T) => a.equal(b)`)
— тип без `@compare` (и без explicit `@equal`) корректно отвергается на
`nova check` вместо того, чтобы резолвер call-site молча подставлял
произвольный одноимённый метод другого типа (UB через `void*`-каст,
verified на сгенерированном C).

Оба изменения — **language-behavior-changing** в строгом смысле «раньше
компилировалось (ложно), теперь отвергается» — но исключительно в сторону
УЖЕСТОЧЕНИЯ ранее необнаруживаемых нарушений языковых правил, уже
объявленных в спеке (D145 Ред.5 / D72 бáунды, D183 default-body). Ни один
существующий std/полярис/примеры файл не пострадал (канон 148/26/61 и
55/0 не сдвинулся).

## Гейты

- `cargo build --release` (`compiler-codegen`, `nova-cli`) — чисто, 0
  ошибок, 62 warning'а (то же число, что и на baseline main, ни одного
  нового от этого окна).
- `nova check std/src` — **148 PASS / 26 FAIL / 61 WARN**, канон держится
  (те же 26 named-neg фикстур).
- `nova-polaris` (`nova.sh check src` со своим бинарём) — **55 PASS / 0
  FAIL**, канон держится.
- `bash scripts/guards/arch-ratchet.sh` — `lines=64444 <= 64444`,
  `infer=348 <= 348` — на потолке baseline, не превышен (фикс в
  чекер-канале, `emit_c.rs` не тронут).
- `nova lint probes-p383` — 0 findings.

## Модель

sonnet (по заданию окна).
