# PROGRESS p248-mech — `#no_copy`: атрибут, реестр с уровнем, транзитивность (волна 1 из трёх)

Модель: sonnet. Волна 1 — ровно четыре вещи (разбор атрибута, признак в
объявлении типа, реестр с уровнем строгости, транзитивность). Проверку
второго имени (D180 Rule 1/2, новые no_copy-диагностики) НЕ трогал — это
волна 2, намеренно не смешана с этой.

Бинарь и сборка: `nova-cli` (`cargo build --release`), бинарь
`d:/Sources/nv-lang/nova-p248m/nova-cli/target/release/nova.exe`. Worktree
`d:/Sources/nv-lang/nova-p248m` (ветка `p248-mech`).

---

## Что и где изменено

### 1. Парсер — восьмая ветка `parse_type_attrs`

`compiler-codegen/src/parser/mod.rs`:
- `:2639` — сигнатура `parse_type_attrs` расширена с 5-кортежа до 6-кортежа
  (добавлен `bool` в конец — `no_copy`, минимизирует правку позиционных
  элементов).
- `:2656` — `let mut no_copy: bool = false;` (локальный аккумулятор, по
  образцу `zero_on_move` на `:2651`).
- `:2810-2823` — ветка `"no_copy" => { ... }`: bare marker, no args,
  duplicate-check (`if no_copy { return Err(...) }`) — байт-в-байт форма
  `"zero_on_move"` ветки (`:2792-2807` после сдвига).
- `:2874` (было `2867`) — `Ok((attrs, impl_protocols, zero_on_move, pub_to,
  serde_attrs, no_copy))`.
- `:1370` (call site) — `let (type_attrs, impl_protocols, zero_on_move_attr,
  pub_to_attr, serde_attrs, no_copy_attr) = self.parse_type_attrs()?;`.
- `:1707-1715` (guard «атрибуты валидны только перед `type`») — добавлен
  `|| no_copy_attr` в условие; текст диагностики дополнен: `` `#from_fields`
  / `#from_pairs` / `#zero_on_move` / `#pub_to` / `#serde` / `#no_copy` are
  only valid before `type` `` — разведка (`PROGRESS-p248-nocopy2.md` §1)
  предупреждала: список имён в matcher'е и текст диагностики — ДВА
  независимых места; оба обновлены синхронно в этом коммите.
- `:1747` — call site `parse_type_decl(..., no_copy_attr, ...)`.
- `:4121` — сигнатура `parse_type_decl` получает параметр `no_copy: bool`
  (вставлен после `serde_attrs`, перед `doc`).
- Три конструктора `TypeDecl { ... }` внутри `parse_type_decl` (Opaque-ветка
  `external type`, empty-sum ветка, основная ветка в конце функции) —
  каждому добавлено поле `no_copy,` рядом с `zero_on_move,`. Подтверждено
  разведкой: их действительно три (Record — основная ветка, `Opaque` —
  `external type`, `Sum(Vec::new())` — empty-sum), не одно.

Других call site'ов `parse_type_attrs`/`parse_type_decl` в файле нет
(проверено грепом до и после правки).

### 2. AST — отдельное bool-поле

`compiler-codegen/src/ast/mod.rs:1232` — `pub no_copy: bool,` рядом с
`zero_on_move` (`:1222`), с доккомментом, объясняющим уровень (D133
таблица, checker-only, wave-1 неподключённость). `TypeDecl` derives
`Default` (`#[derive(Debug, Clone, Default)]`, `:1152`) — новое поле
дефолтится в `false` автоматически. Проверены ВСЕ 20 сайтов
`TypeDecl { ... }`-конструкции в крейте (`emit_c.rs`, `gc_layout.rs`,
`sum_schema_registry.rs`, `lints.rs`, `protocols/auto_derive.rs`,
`protocols/share_check.rs`, `types/mod.rs`, помимо парсера) — все, кроме
парсера, используют `..Default::default()` / `..TypeDecl::default()`,
поэтому новое поле не потребовало правок нигде, кроме самого парсера.

### 3. Реестр — `consume_levels: HashMap<String, ConsumeLevel>`, параллельный `consume_types`

`compiler-codegen/src/types/mod.rs`:
- `:34619-34635` — новый `enum ConsumeLevel { MustConsume, Affine }` с
  доккомментом, объясняющим ДИЗАЙН-РЕШЕНИЕ (см. «Отклонение от буквы
  разведки» ниже).
- `:34637-34674` — `struct LinearityRegistry` получает новое поле
  `consume_levels: HashMap<String, ConsumeLevel>` (`:34653`), доккомменты
  на СТАРОМ `consume_types` (`:34639-34644`) и на новом поле явно
  зафиксированы: `consume_types` — нетронут, только `MustConsume`, читается
  ~20 существующими сайтами; `consume_levels` — питает ТОЛЬКО транзитивный
  обход, Rule 1/2 его не видят.
- `build()` (`:34712-34730`) и `absorb_external()` (`:34909-34922`) —
  заполняют `consume_levels`: `td.consume` → `MustConsume` (тем же именем,
  что и `consume_types`), `else if td.no_copy` → `Affine` (в
  `consume_types` НЕ добавляется).
- `type_is_consume_v` (старая рекурсивная bool-функция) **заменена**
  `type_consume_level_v` (`:34847-34918`) — единый обход возвращает
  `Option<ConsumeLevel>`; `type_is_consume` (`:34803-34805`) стала тонкой
  обёрткой: `matches!(self.type_consume_level(t, module), Some(MustConsume))`
  — коллапсирует новый уровень до СТАРОГО bool байт-в-байт (см. §«Почему
  это нейтрально» ниже). `type_consume_level`/`type_consume_level_v`
  (`:34820-34918`) — публичный вход + visited-guarded рекурсия (та же
  cycle-guard логика, что была: `visited.insert`/`remove` вокруг
  record/sum field-walk).
- `combine_consume_level` (`:34837-34845`) — комбинатор без
  short-circuit: `MustConsume` доминирует над `Affine`, оба — над `None`.
  Использован везде, где раньше был `.any(...)` (generics, record/sum
  fields, tuple elems) — заменён на `.fold(None, |acc, x|
  combine_consume_level(acc, ...))`, чтобы результат не зависел от порядка
  находок (см. unit-тест `must_consume_dominates_affine_when_mixed_in_same_container`).

### 4. Транзитивность — тот же обход, что и раньше, теперь возвращает уровень

Реализована КАК ЧАСТЬ пункта 3 выше (`type_consume_level_v`) — один проход,
не раздвоенный: нет отдельной функции «для уровня» и отдельной «для bool»,
только одна рекурсия + тонкая bool-обёртка поверх неё.

### 5. Rust unit-тесты (верификация, не отдельный пункт волны)

`compiler-codegen/src/types/mod.rs:48938-49010` (после сдвига номеров) —
новый `#[cfg(test)] mod p248_mech_no_copy_tests`, вставлен перед
существующим `primitive_mut_method_tests`, по образцу
`named_tuple_ctor_infer_tests` (парсинг через `crate::parser::parse` +
прямой вызов внутреннего API). Причина Rust-уровня, а не `.nv`-фикстуры:
`ConsumeLevel::Affine` в волне 1 НИКУДА не подключён (никакая диагностика
его не читает) — транзитивность физически не имеет внешнего сигнала для
`nova check`, единственный способ её «увидеть» — прямой вызов
`LinearityRegistry::type_consume_level`.

---

## Дизайн-решение, отклоняющееся от буквы разведки — и почему

`PROGRESS-p248-nocopy2.md` §3.3 рекомендовал **заменить**
`consume_types: HashSet<String>` на `HashMap<String, ConsumeLevel>` (то
есть один реестр вместо двух). Я сделал ИНАЧЕ: добавил `consume_levels`
**параллельно**, оставив `consume_types` нетронутым.

Причина: `consume_types` читается напрямую (`.contains(...)`) в **~20
местах** (`grep -n "consume_types" types/mod.rs` до правки — строки
36867/36920/37978/38505/38880/38883/38902/39356/39462/39538/40817/40849/
40938/41433 и другие), включая ОБА правила D180 (Rule 1
`E_CONSUME_KEYWORD_MISSING`, Rule 2 `E_VIEW_BINDING_FORBIDDEN`). Если бы
`no_copy`-типы попали в тот же контейнер (даже с меткой уровня), Rule 2 —
по находке разведки §3.2 — уже СЕГОДНЯ проверяет `alias_obligated` ПО ИМЕНИ
ТИПА (`consume_types.contains`), не по факту obligation. Значит простое
добавление no_copy-имён в общий реестр немедленно (без единой правки в
Rule 1/2 коде) включило бы enforcement для `#no_copy`-типов — ЭТО и есть
«поведение проверок копирования», трогать которое бриф прямо запретил
волне 1 («до появления правил волны 2 пометка `#no_copy` не должна менять
НИ ОДНОЙ диагностики»).

Решение: `consume_types` остаётся байт-в-байт тем, чем был (только
`td.consume`-имена); `consume_levels` — новая, отдельная карта, которую
сегодня читает ТОЛЬКО `type_consume_level`/`_v` (транзитивный обход).
`type_is_consume`/`type_is_consume_v` (bool API, 2 внешних вызывающих —
`:35012` field-walk и Rule 2 контейнер-наследование `:38984`) —
единственный МОСТ между новым и старым миром, и он математически
эквивалентен старому поведению (см. ниже). Волна 2 подключит
`consume_levels`/`type_consume_level` к Rule 1/2 явной правкой этих мест —
это её работа, не этой волны.

**Почему это байт-в-байт нейтрально:** до этой волны `consume_types`
содержал ИСКЛЮЧИТЕЛЬНО имена `td.consume`-типов. После волны — по-прежнему
исключительно их же (`no_copy`-имена в `consume_types` не добавляются
нигде — ни в `build()`, ни в `absorb_external()`). Значит любой код,
читающий `consume_types` напрямую (все ~20 существующих сайтов), видит
РОВНО ТЕ ЖЕ данные, что и до волны — ноль диффа. `type_is_consume`
(единственная функция, у которой ИЗМЕНИЛАСЬ реализация) коллапсирует новый
`Option<ConsumeLevel>` до bool через `matches!(_, Some(MustConsume))` —
для любого типа, который раньше давал `true` (был найден в
`consume_types` где-то по обходу), новая реализация тоже даёт `true`
(тот же `MustConsume`-путь, та же карта имён под другим ключом типа); для
любого типа, который раньше давал `false` (не найден, включая ВСЕ
сегодняшние типы — `#no_copy` ещё не существовал), новая реализация
тоже даёт `false` — `Affine`-находки коллапсируются в `false`, как и
`None`.

---

## Прогоны — вердикты дословно

### Rust unit-тесты (`cargo test --lib p248_mech_no_copy_tests`, `compiler-codegen/`)

```
running 4 tests
test types::p248_mech_no_copy_tests::no_copy_attr_parses_and_sets_ast_flag ... ok
test types::p248_mech_no_copy_tests::must_consume_dominates_affine_when_mixed_in_same_container ... ok
test types::p248_mech_no_copy_tests::no_copy_propagates_transitively_through_record_field ... ok
test types::p248_mech_no_copy_tests::no_copy_registers_affine_level_directly ... ok

test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 1227 filtered out; finished in 0.00s
```

### `.nv`-пробы (`nova check <dir>`, три отдельных каталога — как требует
раннер, компилирующий все `.nv` папки одним CU)

**probe1_parses** (`#no_copy type Handle value priv { n u32 }` + `ro a =
Handle{...}`, обычное единственное связывание):

```
ok: .../probe1_parses\main.nv

===== SUMMARY =====
PASS: 1  FAIL: 0
exit=0
```

Подтверждает: (а) атрибут разбирается; (б) поведенческая нейтральность —
единственное `ro`-связывание `#no_copy`-типа компилируется БЕЗ ритуала
`consume` (Rule 1 не сработал — `no_copy`-имя не в `consume_types`, как и
задумано).

**probe2_nontype_rejected** (`#no_copy` перед `fn`, не перед `type`):

```
FAIL: .../probe2_nontype_rejected\main.nv
  .../probe2_nontype_rejected\main.nv:4:1: error: `#from_fields` / `#from_pairs` / `#zero_on_move` / `#pub_to` / `#serde` / `#no_copy` are only valid before `type`
  4 | fn stray_marker() -> int { 0 }
    | ^^

===== SUMMARY =====
PASS: 0  FAIL: 1  WARN: 0
error: 1 file(s) failed type-check
exit=1
```

Подтверждает: пометка на не-типе отвергается внятной ошибкой, текст
включает `#no_copy` (guard и его диагностика синхронизированы).

**probe3_duplicate** (`#no_copy` дважды подряд):

```
FAIL: .../probe3_duplicate\main.nv
  .../probe3_duplicate\main.nv:4:1: error: duplicate `#no_copy` attribute
  4 | #no_copy
    | ^

===== SUMMARY =====
PASS: 0  FAIL: 1  WARN: 0
error: 1 file(s) failed type-check
exit=1
```

Бонус сверх требуемого минимума — duplicate-check по образцу
`zero_on_move`.

Пробы НЕ закоммичены в репозиторий (лежат в scratchpad-каталоге сессии,
вне discovery-путей `nova test`) — по образцу `scratch_p248n2/` из
разведки: это одноразовые probe-фикстуры волны 1, не постоянный
spec-conformance тест (для постоянного теста в `spec_tests/conformance`
нужен D-блок в спеке, которого для `#no_copy` пока нет — спека НЕ
трогалась в этой волне, см. «Смежные находки» ниже).

### `cargo build --release` (`nova-cli/`)

Чисто, без единого нового warning/error (полный лог проверен трижды по
ходу волны — после парсер+AST, после реестра+транзитивности, после
unit-тестов). Финальная сборка: `Finished \`release\` profile [optimized]
target(s) in 2m 35s`.

### `nova check std/src` — канон

```
===== SUMMARY =====
PASS: 148  FAIL: 26  WARN: 61
error: 26 file(s) failed type-check
```

**148/26/61 — байт-в-байт канон.** Волна нейтральна на std.

### Ratchet (`bash scripts/guards/arch-ratchet.sh`)

```
arch-ratchet ok: lines=64542 <= 64545
arch-ratchet ok: infer=348 <= 348
```

**lines=64542, infer=348 — байт-в-байт канон.** `emit_c.rs` не тронут
(подтверждено и текстом: `git diff --stat` за всю волну ни разу не
показывал этот файл).

---

## Подтверждение нейтральности

- `consume_types` (HashSet, питает Rule 1/2 и ~20 других сайтов) —
  содержимое не изменилось: `no_copy`-имена в него не добавляются нигде.
- `type_is_consume` (единственная изменившая реализацию функция с
  внешними вызывающими) — доказано эквивалентна старой байт-в-байт (см.
  «Дизайн-решение» выше) + покрыта unit-тестом
  (`no_copy_registers_affine_level_directly` явно ассертит `!type_is_consume(Affine-тип)`).
  `emit_c.rs` не тронут — 0 диффа, подтверждено ratchet'ом.
- `nova check std/src` — 148/26/61, канон.
- Мега-CU и флагман-examples — по брифу, гонит интегратор при приёмке
  (не входит в объём этой волны).

---

## Смежные находки

1. **D-амендмент понадобится волне 2 (не этой).** Атрибут `#no_copy`
   сейчас нигде не описан в `spec/` — ни как D-блок, ни как упоминание.
   Когда волна 2 подключит `Affine`-уровень к Rule 1/2 (реальное
   user-facing поведение — запрет второго имени), это станет
   language-changing слиянием и потребует D-амендмента в `spec/decisions/
   02-types.md` рядом с D133 (расширить таблицу из раздела «МЕХАНИЗМ
   ЗАПРЕТА КОПИРОВАНИЯ — РЕШЁН» плана 248, зафиксировать: имя атрибута
   `no_copy`, семантику `Affine` (≤1 раз, забыть можно), применимые
   `TypeDeclKind` (в этой волне НЕ ограничено — гайд ниже), новый(-ые)
   код(ы) диагностики). Эта волна спеку не трогала (по заданию), но
   доклад по брифу требует явно назвать амендмент — назвал.

2. **Guard по `TypeDeclKind` (аналог `E_ZERO_ON_MOVE_INVALID_KIND`) НЕ
   реализован.** `zero_on_move` имеет отдельную проверку
   (`types/mod.rs`, `td.zero_on_move` → допустимо только для
   `Record`/`NamedTuple`/`Newtype`, иначе `E_ZERO_ON_MOVE_INVALID_KIND`).
   Для `no_copy` такой проверки НЕТ — сегодня `#no_copy` разбирается и
   принимается перед ЛЮБЫМ `TypeDeclKind` (Sum, Effect, Protocol, Alias,
   Opaque, TypeSet — тоже, хотя целевое применение по плану — `value`-
   record). Не входило в перечисленные четыре пункта волны 1 (бриф
   называет только guard «атрибут только перед `type`», не guard «атрибут
   только на определённом ВИДЕ типа») — оставляю как явный пробел для
   решения интегратора: нужен ли `E_NO_COPY_INVALID_KIND`-аналог до/во
   время волны 2, или ограничение вида типа не нужно вовсе (текущая
   транзитивность и так работает для Sum/generic-wrap случаев, что может
   быть намеренно шире, чем `value`-record-only).

3. **`consume`+`no_copy` на одном типе** — синтаксически сейчас ничем не
   запрещено (можно написать `#no_copy\ntype X consume {...}`). В `build()`
   я разрешил конфликт детерминированно: `if td.consume { MustConsume }
   else if td.no_copy { Affine }` — `consume` побеждает молча, `no_copy`
   в этом случае просто НЕ регистрируется. Не диагностируется как ошибка.
   Разведка и бриф не упоминали этот edge case; решение интегратора нужно,
   если он не считает молчаливое старшинство приемлемым (аналог duplicate-
   attribute диагностики, но между ДВУМЯ разными атрибутами).

4. **`combine_consume_level` — недоминирующий `None`.** Реализация НЕ
   short-circuit'ит на первой находке (в отличие от старого `.any()`),
   чтобы гарантировать: `MustConsume`, найденный ПОЗЖЕ `Affine` в обходе
   полей/вариантов/дженериков, не потерялся бы. Покрыто unit-тестом
   (`must_consume_dominates_affine_when_mixed_in_same_container`) —
   единственный способ проверить эту гарантию, раз внешней диагностики
   ещё нет.

---

## Коммиты волны (ветка `p248-mech`, worktree `d:/Sources/nv-lang/nova-p248m`)

1. `p248-mech: #no_copy attribute — parser branch + AST field (wave 1, item 1-2)`
   — парсер + AST-поле (пункты 1 и 2 связаны: правка парсера не
   компилируется без AST-поля, разнести на два коммита без промежуточной
   красной сборки было невозможно).
2. `p248-mech: consume-level registry + transitive propagation (wave 1, item 3-4)`
   — реестр с уровнем + транзитивность (пункты 3 и 4; реализованы одной
   функцией — «не раздваивать обход» — поэтому тоже один коммит).
3. `p248-mech: unit tests for consume-level transitivity (wave 1 verification)`
   — Rust unit-тесты, верификация (не отдельный пункт объёма, но нужна
   для доказательства транзитивности при отсутствии внешнего сигнала).
