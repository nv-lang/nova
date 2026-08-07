# PROGRESS — p386-bound-doors

Окно: p386-bound-doors. Модель: sonnet (эффорт средний, суб-агенты не спавнились).
Задача: закрыть КЛАСС «проверка бáунда типового параметра» — реестр
[docs/plans/221.1-bug-sweep.md](../221.1-bug-sweep.md) №386 (4-я дверь: бáунд
на декларации типа) и №388 (5-я дверь: allowlist примитивов), в контексте
№383/№384 (1-я/2-я/3-я двери, уже закрыты).

Worktree: `d:/Sources/nv-lang/nova-p386`, ветка `p386-bound-doors`.

## Шаг 1 — инвентаризация: где механизм «бáунд-проверки» живёт в компиляторе

Единственный узел, который РЕАЛЬНО решает «удовлетворяет ли `concrete` типу
`bound`» — `BoundCtx::check_satisfaction` / `check_satisfaction_against_methods`
(`compiler-codegen/src/types/mod.rs`). До этого окна в него вело ДВА входа
(и оба — call-site, не decl-site):

| Вход (Rust-функция) | Что покрывает | До окна | После окна |
|---|---|---|---|
| `check_call_bounds` → `check_generic_bounds_for_call` | свободная функция `fn[T Bound] f(...)` — 1-я дверь | ✅ закрыто №383 | без изменений |
| `check_method_call_bounds` → `check_generic_bounds_for_call` | метод `fn Recv @m[S Bound](...)` (method-OWN typevar) + receiver-linked `fn[T Bound] []T @m` — 2-я/3-я двери | ✅ закрыто №383 | без изменений |
| — | **бáунд на декларации ТИПА** `type Box[K Bound] {...}` в ЛЮБОЙ позиции инстанцирования — 4-я дверь | ❌ НЕ ПРОВЕРЯЛСЯ НИГДЕ | ✅ закрыто (4 новых call-сайта, см. ниже) |
| `check_satisfaction`/`check_satisfaction_against_methods`'s primitive-blanket | примитив в позиции бáунда — 5-я дверь | ❌ `if is_primitive { return; }` — ЛЮБОЙ бáунд пропускался целиком | ✅ закрыто (структурная сверка + explicit `#coerce`-мост) |

### Полная таблица форм-носителей (что просили в брифе)

| Форма-носитель | Место в коде (ДО окна) | Покрыта ДО | Покрыта ПОСЛЕ | Как |
|---|---|---|---|---|
| Свободная функция `fn[T Bound] f(x T)` | `check_call_bounds` | ✅ | ✅ | без изменений (№383) |
| Метод, receiver-linked typevar `fn[T Bound] []T @m` | `check_method_call_bounds` | ✅ | ✅ | без изменений (№383) |
| Метод, method-OWN typevar `fn Recv @m[S Bound](...)` | `check_method_call_bounds` → arg-inference | ✅ | ✅ | без изменений (№383) |
| **Декларация типа**, инстанцирование через RecordLit-литерал с ИНФЕРЕНСОМ generic-арга из поля (`Box { k: V }`, без явных скобок) | — | ❌ | ✅ | новая `check_record_lit_decl_bounds`, вызов из `walk_expr`'s `RecordLit` arm |
| **Декларация типа**, явный turbofish-конструктор (`Type[ConcreteArgs].new(...)`) | — | ❌ | ✅ | новая ветка в `walk_expr`'s `TurboFish` arm |
| **Декларация типа**, аннотация переменной (`ro x Type[Args] = ...`) | — | ❌ | ✅ | новая `check_typeref_bounds`, вызов из `walk_stmt`'s `Stmt::Let` arm |
| **Декларация типа**, параметр/возврат функции (`fn f(x Type[Args]) -> Type[Args]`) | — | ❌ | ✅ | `check_typeref_bounds`, вызов из `check_module`'s `Item::Fn` (по КАЖДОМУ `f.params`/`f.return_type`) |
| **Декларация типа**, поле записи обобщённого типа (`type Foo { m Type[Args] }`) | — | ❌ | ✅ | `check_typeref_bounds`, вызов из `check_module`'s новой `Item::Type`-ветки (по field/named-tuple/sum-variant типам) |
| Алиас (`type X alias Type[Args]`) | — | ❌ | ✅ (структурно) | та же `Item::Type`-ветка (`TypeDeclKind::Alias`/`Newtype` inner) |
| Вложенный generic `Box[Box[T]]` | — | ❌ | ✅ | `check_typeref_bounds` рекурсирует в `generics` ПЕРЕД собственной проверкой — бесплатно на любой глубине |
| Возвращаемый тип | — | ❌ | ✅ | тот же `f.return_type`-вызов, что и параметр |
| Параметр-замыкание / `fn(...)`-тип, несущий обобщённый тип | — | ❌ | ✅ (структурно) | `check_typeref_bounds`'s `TypeRef::Func` arm рекурсирует в params/return |
| Type-set бáунд (`[T Ints]`, D310) в каждой из этих позиций | `check_satisfaction`'s type-set ветка (до primitive-блока) | ✅ (уже было) | ✅ | не тронуто — D310-ветка стоит РАНЬШЕ primitive-блока, как и раньше |
| **Примитив в позиции ЛЮБОГО бáунда** | primitive-blanket `return` | ❌ (пропускался целиком) | ✅ структурно, кроме 9 builtin-протоколов (см. ниже) | правка `check_satisfaction`/`check_satisfaction_against_methods` |
| Примитив в позиции builtin-протокола (Equal/Hash/Compare/Clone/Display/Debug/Serialize/Deserialize/Reflect) | — | ✅ (случайно, через blanket) | ✅ (осознанно, через `is_builtin_protocol`) | та же правка — узкое исключение вместо широкого |
| `str` → `AsSlice[u8]` через `#coerce str@bytes()` | — | ✅ (случайно, через blanket) | ✅ (осознанно, через новый explicit coerce-мост) | новое поле `coerce_output` + fallback в `check_satisfaction_against_methods` |

**Не применимо / вне области этого окна:**
- Протокольные бáунды на самой протокол-декларации (`type P[T] protocol {...}` c бáундом на `T` протокола) — такой синтаксис в грамматике не встретился, не проверялся отдельно.
- Explicit `Type[Args] { fields }` (RecordLit с явными скобками-generics) —
  **НАХОДКА, не дверь**: это вообще не парсится как задумано (см. ниже).

## Шаг 2 — сведено ли к одному входу

Да, для САМОЙ проверки: и старые (doors 1-3), и все новые (door 4, оба
под-механизма) call-сайты в итоге вызывают ОДНУ и ту же пару
`check_satisfaction`/`check_satisfaction_against_methods`. Это тот же паттерн,
что уже был у №383 (`check_call_bounds` и `check_method_call_bounds` — ДВА
входа в ОДИН `check_generic_bounds_for_call`) — не шестая ветка, а ещё
несколько ВХОДОВ в тот же единственный решатель.

**Что НЕ удалось свести в один вход и почему** (честно, не то же самое, что
«готово»):
1. **`TypeCheckCtx` и `BoundCtx` — два независимых прохода.** Арность/
   существование типа (`walk_typeref`, `E_TYPE_ARITY_MISMATCH`) живёт в
   `TypeCheckCtx`; сатисфакция бáундов — в `BoundCtx`. Это ДОкомпилятор-
   архитектурное разделение (разные структуры, разные `impl`-блоки, разные
   моменты запуска), не тронутое этим окном — попытка слить их в один проход
   выходит далеко за рамки «класс, а не случай» и требует отдельного плана
   (ближе к rustc-эталону: единый resolve-проход). Поэтому у меня ДВЕ
   реализации одного и того же рекурсивного обхода `TypeRef` —
   `TypeCheckCtx::walk_typeref` (арность) и новая `BoundCtx::check_typeref_bounds`
   (бáунды) — структурно похожие, физически разные функции в разных `impl`.
   Дублирование обхода дерева типов, не дублирование ЛОГИКИ проверки.
2. **RecordLit-инференс (`check_record_lit_decl_bounds`) — отдельная функция**,
   не влитая в `check_typeref_bounds`, потому что источник данных другой:
   `check_typeref_bounds` видит ЯВНО НАПИСАННЫЙ `TypeRef`; RecordLit без
   скобок никакого `TypeRef` с generic-аргументами в исходнике не содержит —
   generic-арг ВЫВОДИТС� из типа поля-литерала (см. `TypeCheckCtx::f1_expr_inner`'s
   `gen_args`-инференс, который я НЕ смог переиспользовать напрямую — он живёт
   в другом проходе и пишет в другой канал, `resolved_types_buf`). Пришлось
   переимплементировать ТУ ЖЕ логику инференса внутри `BoundCtx` (комментарий
   в коде это явно фиксирует).

## Пробы «подсунь заведомо негодное» + обратные — по каждой ПОКРЫТОЙ форме

Все 12 фикстур лежат в `spec_tests/conformance/` (позитив) и
`spec_tests/conformance/neg/` (негатив), проверены `nova check` ИНДИВИДУАЛЬНО
(не мега-CU — см. CPU-дисциплину ниже) + `nova lint` (0 находок на всех 12).

| Форма | Негатив-фикстура | Код ошибки (точный) | Позитив-фикстура |
|---|---|---|---|
| RecordLit-инференс | `neg/p386_type_decl_bound_recordlit_neg.nv` | `type \`Widget386\` does not satisfy \`NeedsFoo386\` bound` | `p386_type_decl_bound_recordlit_pos.nv` |
| Явный turbofish-конструктор | `neg/p386_type_decl_bound_turbofish_neg.nv` | `type \`Widget386TF\` does not satisfy \`NeedsFoo386TF\` bound` | `p386_type_decl_bound_turbofish_pos.nv` |
| let-аннотация | `neg/p386_type_decl_bound_annotation_neg.nv` | `type \`Widget386Ann\` does not satisfy \`NeedsFoo386Ann\` bound` | `p386_type_decl_bound_annotation_pos.nv` |
| Параметр функции (сигнатура, БЕЗ вызова) | `neg/p386_type_decl_bound_fnsig_neg.nv` | `type \`Widget386Fn\` does not satisfy \`NeedsFoo386Fn\` bound` | `p386_type_decl_bound_fnsig_pos.nv` |
| Поле записи обобщённого типа | `neg/p386_type_decl_bound_field_neg.nv` | `type \`Widget386Fld\` does not satisfy \`NeedsFoo386Fld\` bound` | `p386_type_decl_bound_field_pos.nv` |
| Примитив, дословная проба владельца (`[]u8.append(42)`) | `neg/p386_primitive_structural_bound_neg.nv` | `type \`int\` does not satisfy \`AsSlice\` bound` | `p386_primitive_structural_bound_pos.nv` (Hash-примитив + `Vec[u8]`-AsSlice + `str`-через-`#coerce`) |

Диагностика — БЕЗ нового `E_*`-кода: это та же безымянная строка «type `X`
does not satisfy `Y` bound…», что уже эмитит `check_satisfaction_against_methods`
для дверей 1-3 (№383). Правило 4 (neg-фикстура на каждый НОВЫЙ `E_*`-код) не
триггерится — страж `check-test-fixture-coverage.sh` правило 5 грепает
добавленные `"E_..."`/`"W_..."` строковые литералы в диффе; я не добавил ни
одного. `E_TYPE_NOT_IN_SET` (D310, type-set-бáунды) не тронут — его ветка
стоит РАНЬШЕ primitive-блока в `check_satisfaction`, как и раньше.

Обратные пробы (законный тип НЕ отвергается) — тот же файл (каждый
позитив-файл содержит `assert(...)` на реально прошедшем значении, не только
«компилируется»).

## Дверь 5 отдельно — не просто «убрал allowlist»

Первая попытка («примитив просто идёт в общий структурный путь») СЛОМАЛА
регресс `nova check std/src`: 148/26 → множество новых FAIL:
- `HashMap[K, V].new()` и любой метод `HashMap`/`IndexMap`, не
  РЕСТЕЙТЯЩИЙ бáунд ресивера (`fn HashMap[K, V].new(...)` — тип ДЕКЛАРИРУЕТ
  `K Hash`, но конкретно ЭТОТ метод бáунд не повторяет) — ложный отказ.
- `type HashMapIter[K, V] { map HashMap[K, V] }` и **`type Set[T] { use map
  HashMap[T, ()] }`** — `set/core.nv`'s ШАПКА **явно документирует** это как
  открытый вопрос («T должен быть hashable... Bound'ов в MVP нет
  ([Q-bounds])... компилятор проверит при использовании») — не случайная
  дыра, а сознательное упрощение MVP.

Фикс: `is_passthrough_typevar` — любой БАЗОВЫЙ typevar-имя, уже присутствующее
в `current_fn_gs` (текущий scope generic-параметров — функции ИЛИ, при обходе
`Item::Type`, самой декларации типа), не проверяется этим НОВЫМ door-4/5
кодом — он абстрактный, его когда-нибудь подставит внешний вызывающий, и ЭТА
подстановка уже проверяется в СВОЁМ собственном call-сайте. Это НЕ ослабляет
существующую (до-окна) проверку — она таким типвар-паспортом не занималась
никогда; это just-in-scope для НОВОГО кода. Итог: `nova check std/src`
осталось РОВНО 148/26/61 (проверено дважды, до и после финальной правки).

Отдельно (по-настоящему тонкое место): allowlist изначально включал ЛЮБОЙ
бáунд для примитива. Узкое сужение до 9 builtin-протоколов
(`is_builtin_protocol`: Equal/Hash/Compare/Clone/Display/Debug/Serialize/
Deserialize/Reflect) переиспользует УЖЕ СУЩЕСТВУЮЩУЮ авторитетную семантику
(`auto_derive::check_field_eligibility`'s `is_primitive_type(name) => true`
безусловно для ЭТОГО ЖЕ семейства протоколов) — не новое правило, применение
существующего в новом месте.

### Побочная находка внутри двери 5: `#coerce` — законный путь, оформлен явно

Первая версия узкого фикса ЛОМАЛА реальную, ВЕРИФИЦИРОВАННУЮ в прошлом окне
p383 пробу `probes-p383/p383_coerce_asslice_pos/main.nv` (`v.append("hello")`
без явного `.bytes()`) — она явно документирует, что `str`'s прохождение
`AsSlice[u8]` держалось ИСКЛЮЧИТЕЛЬНО на blanket-скипе, а РЕАЛЬНЫЙ мост
(`#coerce str@bytes()`, вставляется кодогеном в позиции аргумента) чекеру
бáундов был не виден вообще. Добавлено: `coerce_output` (BoundCtx) —
облегчённый пересбор `#coerce`-пар (тип-ресивер → тип-результат, БЕЗ полной
D429 R1-R15 валидации — она уже есть в `TypeCheckCtx`) + fallback в
`check_satisfaction_against_methods`: если структурная проверка ПРЯМОГО типа
провалилась, но у него есть `#coerce`-цель, удовлетворяющая бáунду — пропуск.
Один хоп, без цепочек (D429 сам не допускает chaining). Работает для ЛЮБОГО
типа с `#coerce`, не только примитивов — соответствует требованию брифа
«`#coerce` — отдельный законный путь, оформить явно».

**Найдено по вопросу владельца (не чинится в этом окне — за скобками)**:
`Type[Args] { fields }` (RecordLit с ЯВНЫМИ generic-скобками) вообще не
парсится как задумано — `Box[NoMethods] { k: ... }` превращается в
`Index{Box, NoMethods}` (bogus) + ОТДЕЛЬНЫЙ dangling anonymous record-literal,
падающий на кодогене с «anonymous record literal without spread not
supported». Дословная форма пробы владельца из брифа (`Box[NoMethods] { k:
NoMethods{x:1} }`) поэтому реально проверяет parser-gap, а не door 4 —
door 4 подтверждён РЕАЛЬНОЙ, рабочей формой `Box { k: NoMethods{x:1} }`
(инференс из поля, без явных скобок). Отдельная, тоже реальная находка —
не трогалась (вне мандата этого окна, не door 4/5).

## Находки в std (не правил, доложил)

- `std/src/collections/set/core.nv`: `Set[T]` — бáунда на `T` нет по дизайну
  (комментарий в файле, Q-bounds). Дверь 4/5 туда СТРУКТУРНО могла бы
  дотянуться (через `use map HashMap[T, ()]`), но намеренно не дотягивается
  (passthrough-guard) — иначе ужесточение сломало бы MVP-упрощение, о котором
  владелец не просил в этом окне.
- `std/src/collections/hash_map/core.nv`, `index_map/core.nv`: методы,
  оперирующие СВОИМ receiver-типом (`HashMap[K, V].new()`, `@clone()` и т.д.),
  не всегда рестейтят бáунд ресивера дословно — тоже passthrough, тоже не
  ошибка, просто ещё один пример того же паттерна.
- **№389/№407 смежность**: мой фикс НЕ уменьшает счётчик хардкода имён типов
  (`"str"`/`"Vec"`/`"Channel"` по строке) — `is_builtin_protocol`,
  `is_primitive_type`, `type_decls`/`coerce_output` в этом окне добавлены КАК
  НОВЫЕ карты по .nv-декларациям (не хардкод по строке типа) — в этом смысле
  окно НЕ добавило вхождений в проблему №389, но и не убрало ни одного из
  существующих 184/170/etc — оценка "на сколько уменьшает": 0 (не трогал те
  файлы).

## Регресс

`nova check std/src` (worktree, после ВСЕХ правок): **PASS: 148 FAIL: 26
WARN: 61** — байт-в-байт канон. Список 26 FAIL-файлов не сверялся построчно
(числа совпали трижды подряд на разных стадиях правки — до primitive-фикса,
после passthrough-guard, после coerce-моста — устойчивый сигнал отсутствия
регресса).

`nova lint` на всех 12 новых фикстур: 0 находок.

`cargo build --release` (nova-cli): чисто, только pre-existing warnings (dead
code/unused vars, не мои).

## CPU-дисциплина

Мега-CU (`spec_tests/conformance` целиком) НЕ гонялся — по инструкции
(авторитетный гейт у интегратора). Проверено: `cargo build --release`
(nova-cli), `nova check std/src`, `nova check` индивидуально на каждой из 12
новых фикстур + repro-файлах (каждый прогон внутри `spec_tests/conformance/`
неизбежно тянет соседние файлы как peer — это НЕ полный мега-CU прогон, но и
не изолированный single-file check; ~40с на файл, объяснимо структурой пакета).

## Файлы

Компилятор: `d:/Sources/nv-lang/nova-p386/compiler-codegen/src/types/mod.rs`
— единственный тронутый файл (454 добавленных / 8 удалённых строк):
- `BoundCtx` — новые поля `type_decls`, `coerce_output`.
- `BoundCtx::build` — их сбор (local + peer files, тот же паттерн, что
  `impl_protocol_types`/`value_type_names`).
- Новые методы: `check_typeref_bounds`, `check_record_lit_decl_bounds`,
  `is_passthrough_typevar`, `turbofish_base_name`.
- Новые вызовы: `walk_expr`'s `RecordLit`/`TurboFish` arms; `walk_stmt`'s
  `Stmt::Let`; `check_module`'s `Item::Fn` (params/return) и новая
  `Item::Type` ветка (fields/named-tuple/sum-variants/newtype/alias).
- `check_satisfaction`/`check_satisfaction_against_methods` — primitive-блок
  сужен с «пропустить всё» до «пропустить только 9 builtin-протоколов +
  `any`/`never`», плюс новый `#coerce`-fallback.
- `infer_arg_ty` (BoundCtx's lightweight scope-инференс) — новая ветка для
  `[]T.new()` (parses as `Path(["__array", T])` + `Member` + `Call`) — без
  неё дословная проба владельца (`mut v = []u8.new(); v.append(42)`) вообще
  не попадала в scope и молчала независимо от primitive-фикса (доказано:
  тот же silent-pass воспроизводился и с заведомо плохим ПОЛЬЗОВАТЕЛЬСКИМ
  типом, не только с примитивом).

Фикстуры (12, все под `spec_tests/conformance/`):
`p386_type_decl_bound_{recordlit,turbofish,annotation,fnsig,field}_pos.nv`,
`p386_primitive_structural_bound_pos.nv`,
`neg/p386_type_decl_bound_{recordlit,turbofish,annotation,fnsig,field}_neg.nv`,
`neg/p386_primitive_structural_bound_neg.nv`.

Repro (владельца, для истории): `docs/plans/repro/p386_probe1_type_decl_bound.nv`,
`docs/plans/repro/p386_probe2_primitive_allowlist.nv`.

## Что НЕ сделано / осталось открытым

1. `TypeCheckCtx`/`BoundCtx` — два прохода, дублирующие обход `TypeRef` (см.
   Шаг 2). Слияние — отдельный план.
2. `Type[Args] { fields }` explicit-скобки на RecordLit — parser-gap,
   отдельная находка (задокументирована выше), не door 4/5.
3. Протокольная бáунд-декларация внутри protocol-типа с бáундом на ЕГО
   собственном типовом параметре — синтаксис не встретился, не проверялся.
4. №389 (хардкод имён типов) / №407 (Channel по строке) — НЕ уменьшены этим
   окном (см. «Находки в std»).
5. Спек-амендмент: это ужесточение диагностики (было тихо — стало явной
   ошибкой), не языковая семантика — новых конструкций/D-решений не вводит,
   амендмент не требуется (сверено с D72/D145/D310 — все три уже описывают
   ИМЕННО это требование «бáунд обязан проверяться», этот код просто
   перестал его молча нарушать в ранее непроверенных позициях).
