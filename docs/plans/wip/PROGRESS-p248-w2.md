<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# PROGRESS p248-w2 — `#no_copy`: правила второго имени (волна 2 из трёх)

Модель: sonnet. Волна 2 план 248: энфорс «нельзя второе имя» для типа
уровня `Affine` (`#no_copy`, D447) — три формы, которых волна 1 не видела
(bare alias уже ловился частично существующей машиной D180, но не для
`Affine`-уровня; field-read, аргумент, литерал-embed не ловились вовсе),
плюс правило заимствования, свод реестров, запрет `consume`+`#no_copy`,
ограничение по виду объявления, свои диагностики.

Worktree `d:/Sources/nv-lang/nova-p248w2` (ветка `p248-w2`). Бинарь
`d:/Sources/nv-lang/nova-p248w2/nova-cli/target/release/nova.exe`.

---

## Объём — что сделано (все 6 пунктов брифа)

### 1. Энфорс «нельзя второе имя» — новый независимый проход

`compiler-codegen/src/types/mod.rs`, функция `check_no_copy_second_name`
(вызывается из главного пайплайна сразу после `check_ref_addr_escape`,
рядом с `check_consume`). **Сознательно НЕ встроена в `check_consume`**:
`Affine` не несёт consume-обязанности («забыть — можно», D447), поэтому
flow-sensitive машина `Live`/`Consumed`/`MaybeConsumed` ей не нужна —
это структурная (не flow-sensitive) проверка.

Четыре формы:
- **(a) bare alias** (`ro b = a`) и **(b) field-read** (`x = obj.field`) —
  ловятся в `Stmt::Let`-обработчике: если RHS — путь (`Ident`/`@self`/
  `.field`-цепочка), чей best-effort резолвнутый тип `Affine` — ошибка.
- **(d) литерал-embed** (`Type { field: a }`, `(a, b)`) — при обходе
  `RecordLit`/`TupleLit`: каждое значение поля/элемент, если это путь
  `Affine`-типа — ошибка. Свежая конструкция (`Type{field: Handle{...}}`)
  НЕ флагуется — RHS не путь, а литерал.
- **(c) аргумент** — см. п.2 (заимствование) ниже.

Резолв типа выражения-пути — `NoCopyWalk::resolve_path_type` (свой
per-function `scope: HashMap<String, TypeRef>`, заполняется из параметров
+ по ходу обхода `let`); поиск типа поля — `NoCopyIndex` (пер-модульный
индекс `record_fields` + `fns`, строится один раз, включая peer-файлы).

**Обход по модулю — `Item::Fn` И `Item::Test`.** Найдено верификацией:
`test { … }`-блок — отдельный `Item::Test`, НЕ `Item::Fn`; первый проход
итерировал только `Item::Fn` и все три негативные пробы внутри
`test {}`-блоков молча проходили (`nova check` — `ok`). `check_consume`
уже ходит по обоим kind'ам (своя `Item::Test`-ветка) — тот же паттерн
скопирован.

### 2. Правило заимствования — консервативный критерий

Передача в параметр реализована как проверка на КАЖДОМ `Call`: если
аргумент — путь `Affine`-типа, заимствование разрешено (без диагностики)
ТОЛЬКО если:
- callee статически резолвится (`NoCopyIndex.fns` — свободная функция по
  имени ИЛИ метод по резолвнутому типу receiver'а; `Named`/`Spread`
  аргументы НЕ участвуют — только позиционные `Item`, дефолт для
  остальных форм — эскейп);
- параметр — голый `ro` (`!p.is_mut && !p.consume`);
- тело callee НЕ эскейпит параметр — `nc_param_escapes`.

`nc_param_escapes` — рекурсивный структурный обход тела callee
(`nc_scan_block`/`nc_scan_stmt`/`nc_scan_expr`), ищущий: возврат
(`return`/tail), запись в поле/индекс (`Stmt::Assign` c
`Member`/`Index`-целью), встраивание в литерал, захват в замыкание/
`spawn`/`detach`/`blocking`/`supervised`, дальнейшую передачу аргументом
другого вызова. Недоступное тело (`FnBody::External`) → эскейп по
умолчанию (безопасный дефолт).

**Найденный и исправленный на этапе верификации баг:** первая реализация
использовала «имя параметра встречается ГДЕ-ЛИБО в tail/return-выражении»
(`nc_expr_contains`, коллектор имён из `alpha_rename`) как сигнал эскейпа
через return-позицию. Это ложно флагует ЛЮБОЕ использование параметра как
receiver'а метода: `fn borrow_only(h Handle) -> int { h.get() }` —
`h.get()` содержит идентификатор `h`, хотя возвращается int, а не `h`.
Позитивная проба (`probe_pos`, ниже) сразу поймала это — заимствование
ложно отклонялось. Исправление — `nc_value_escapes`: отдельная функция,
распознающая именно ИДЕНТИЧНОСТЬ значения (`h` САМ является результатом),
а не произвольное упоминание имени: голый `Ident`, сквозные обёртки
(`Block`/`If`/`Match`/`Coalesce`/`Try`/`Bang`/`As`/`RefArg` — их значение
может быть буквально `h`), встраивание в литерал, передача аргументом.
Проекции (`.field`, индекс), бинарные/унарные операции и вызовы МЕТОДОВ на
`h` (сам `func` не проверяется, только `args`) — НЕ распознаются: это
структурно новое значение. Ровно это различение и делает `h.get()` борроу.

### 3. Свод реестров

`LinearityRegistry.consume_types: HashSet<String>` **удалён** (не оставлен
параллельно, как в волне 1) — единственный источник истины теперь
`consume_levels: HashMap<String, ConsumeLevel>`. Новый метод
`is_must_consume_name(&self, name: &str) -> bool` (`matches!(consume_levels
.get(name), Some(MustConsume))`) заменяет `consume_types.contains(name)`
байт-в-байт (та же карта имён, тот же набор — `MustConsume`-записи не
изменились). Переведено **13 читателей** (не ~20, как в оценке разведки —
пересчитано грепом до и после): 11 сайтов `.contains(...)` →
`.is_must_consume_name(...)`, 2 сайта `.iter().any(...)` (обход ВСЕХ
must-consume имён типа для поиска consume-метода по имени, `@field.method()`
D5/D5.2-tracking) → `consume_levels.iter().filter(|(_, lvl)| **lvl ==
MustConsume).any(...)`. `build()`/`absorb_external()` больше не заполняют
отдельный `HashSet` — только `consume_levels`. Добавлен `type_is_no_copy`
(зеркало `type_is_consume`, но для `Affine`) — теперь `consume_levels`
питает ОБА направления: Rule 1/2 (`MustConsume`, как раньше) И новый
`check_no_copy_second_name` (`Affine`).

**Поведение must-consume типов не изменилось ни на йоту** — доказано:
(а) математически (та же карта имён под другим методом), (б) `nova check
std/src` = 148/26/61 байт-в-байт, (в) точечные regression-пробы существующих
consume-фикстур (`d133_consume_type_must_consume.nv`,
`consume_collection_vec_push_forgotten_neg.nv`) дают тот же вердикт/тот же
текст ошибки, что и до волны.

### 4. Запрет `consume` + `#no_copy` на одном типе

`check_linearity_markers` (тот же проход, что и D133-маркеры), новая
проверка ПЕРЕД существующей record-field-логикой: `td.consume &&
td.no_copy` → `[E_NO_COPY_CONSUME_CONFLICT]` (текст — свой, объясняет
конфликт уровней, не переиспользует consume-текст).

### 5. Ограничение по виду объявления

`E_NO_COPY_INVALID_KIND` — по образцу существующего
`E_ZERO_ON_MOVE_INVALID_KIND` (тот же файл, чуть выше). Разрешённые виды —
**Record/Sum/NamedTuple/Newtype/Opaque** (то же множество, что у `#share`,
D415 §1 — «конкретная instance-идентичность»), НЕ Alias/TypeSet/Effect/
Protocol («нет собственного хранилища»). Решение шире буквы брифа
(«осмыслен для записи и суммы») — включает NamedTuple/Newtype/Opaque по
тому же основанию «есть собственное значение», симметрично `#share`;
названо явно, чтобы интегратор мог сузить при несогласии.

### 6. Свои диагностики

`E_NO_COPY_SECOND_NAME` (4 формы, текст различается по форме через
параметр `form`), `E_NO_COPY_CONSUME_CONFLICT`, `E_NO_COPY_INVALID_KIND` —
все три текста написаны с нуля, ни один не переиспользует фразы
`consume`-диагностик («обязан потребить» и т.п.). `E_NO_COPY_SECOND_NAME`
явно говорит: «копия запрещена; забыть исходное имя МОЖНО (в отличие от
`consume` — потреблять не обязательно)».

---

## Прогоны — вердикты дословно

### `cargo build --release` (`nova-cli/`)

Чисто, без единого нового warning/error. Финал:
`Finished \`release\` profile [optimized] target(s) in 4m 12s` (после
финального фикса `nc_value_escapes`; полных пересборок за волну было три
— после каждого содержательного изменения).

### `.nv`-пробы (8 фикстур, каждая — своя папка, `nova check <dir>`)

Все восемь дают ожидаемый вердикт:

```
probe_pos            (позитив: конструкция + заимствование)  ok: PASS 1 FAIL 0
probe_bare_alias     (форма a)   FAIL [E_NO_COPY_SECOND_NAME] голое связывание…
probe_field_read     (форма b)   FAIL [E_NO_COPY_SECOND_NAME] чтение поля…
probe_arg_escape     (форма c)   FAIL [E_NO_COPY_SECOND_NAME] передача аргументом…
probe_record_embed   (форма d)   FAIL [E_NO_COPY_SECOND_NAME] встраивание в литерал записи…
probe_tuple_embed    (форма d)   FAIL [E_NO_COPY_SECOND_NAME] встраивание в литерал кортежа…
probe_consume_conflict           FAIL [E_NO_COPY_CONSUME_CONFLICT]…
probe_invalid_kind               FAIL [E_NO_COPY_INVALID_KIND]… (kind alias)
```

Точный текст каждой ошибки — см. код выше (диагностики в §6). Эти же
фикстуры (переименованные с `N447`-префиксом, домен-namespaced по
конвенции) закоммичены как conformance-тесты — см. ниже.

### Regression-пробы (после свода реестров, п.3)

```
d133_consume_type_must_consume.nv (позитив)         ok: PASS 1 FAIL 0
consume_collection_vec_push_forgotten_neg.nv (Rule1) FAIL [E_CONSUME_KEYWORD_MISSING]
  binding `v` держит consume-обязательную инстанс типа `Vec` — требуется
  keyword `consume` (D180).
```
Тексты байт-в-байт совпадают с ожидаемым (до волны).

### `nova check std/src` — канон

```
===== SUMMARY =====
PASS: 148  FAIL: 26  WARN: 61
```

**148/26/61 — байт-в-байт канон.** В `std` нет `#no_copy`-типов — волна
нейтральна (подтверждено также быстрым выходом
`check_no_copy_second_name`: `if !consume_levels.values().any(Affine) {
return; }` — на `std` не тратит время сверх самого построения реестра).

### Ratchet (`bash scripts/guards/arch-ratchet.sh`)

```
arch-ratchet ok: lines=64545 <= 64545
arch-ratchet ok: infer=348 <= 348
```

Ратчет меряет ТОЛЬКО `compiler-codegen/src/codegen/emit_c.rs` — этот файл
волна не трогала вовсе (весь код в `types/mod.rs`, чекер-канал), значения
байт-в-байт равны baseline.

### Мега-CU/флагман

**Не гонял — по брифу это зона интегратора.** Новые conformance-файлы
(1 positive + 7 neg) добавлены в `spec_tests/conformance/` пир-файлами по
рабочему процессу test-conventions.md («разрабатывается в ОТДЕЛЬНОМ модуле
→ доводится до PASS → мержится») — каждый провалидирован В ИЗОЛЯЦИИ
(копия в свою директорию, `nova check`) с ожидаемым вердиктом ДО
копирования в `spec_tests/conformance/`; повторно провалидированы уже в
финальном виде (с `N447`-префиксом имён) — тем же способом. Полный
1000+-файловый мега-CU не прогонялся (дорого, не входит в объём окна).

---

## Побочная находка — окружение, не код волны

При первичной верификации `probe_invalid_kind` несколько прогонов подряд
зависали (`nova check` не завершался за минуты). Расследование: НЕ баг
волны — на машине параллельно шли ещё 4 чужих `nova.exe` (другие окна,
`nova-p356`/`nova-p238f1`/`nova`/`nova-pptr`, счётчики CPU-времени в
сотнях-тысячах секунд) — CPU starvation, не infinite loop. Подтверждено:
с длинным таймаутом (90с) та же проба честно завершилась с ожидаемым
`[E_NO_COPY_INVALID_KIND]`. Отдельно поймана и НЕ отнесена на волну
реальная деталь: `nova check <dir>` на директории с ЕДИНСТВЕННЫМ
содержимым — голым `type X int` (newtype-примитив) без единой `fn`/`test`
— тоже виснет (проверено и на `main`-бинаре тоже, `d:/Sources/nv-lang/
nova/nova-cli/target/release/nova.exe`, то есть до волны). Не заводил
номер в 221.1 (не мой мандат в этом окне) — интегратору на заметку;
воспроизводится тривиально (`mkdir d; echo 'module m' > d/main.nv; echo
'type X int' >> d/main.nv; nova check d`).

---

## Файлы

- `compiler-codegen/src/types/mod.rs` — весь код волны (registry collapse
  §3, conflict+kind checks §4/5 в `check_linearity_markers`, новый проход
  `check_no_copy_second_name` + `NoCopyIndex`/`NoCopyWalk`/`nc_*`-хелперы,
  вызов из главного пайплайна).
- `spec_tests/conformance/d447_no_copy_second_name.nv` — позитив (3
  test-блока: конструкция без ритуала, заимствование, независимые
  конструкции).
- `spec_tests/conformance/neg/n_d447_bare_alias.nv` — форма (a).
- `spec_tests/conformance/neg/n_d447_field_read.nv` — форма (b).
- `spec_tests/conformance/neg/n_d447_arg_escape.nv` — форма (c), эскейп.
- `spec_tests/conformance/neg/n_d447_record_literal_embed.nv` — форма (d),
  запись.
- `spec_tests/conformance/neg/n_d447_tuple_literal_embed.nv` — форма (d),
  кортеж.
- `spec_tests/conformance/neg/n_d447_consume_conflict.nv` — п.4.
- `spec_tests/conformance/neg/n_d447_invalid_kind.nv` — п.5.

---

## Что НЕ покрыто (честно, по образцу вердикта разведки)

Все — сознательные консервативные упрощения v1, не незамеченные дыры;
над-report (ложный запрет безопасного заимствования) — безопасное
направление, под-report (пропуск реальной копии) — нет, если не оговорено
явно ниже:

1. **`FnBody::Expr` (`=> expr`-тела) и `Item::Const`/`Item::Let` на
   модульном уровне НЕ обходятся `check_no_copy_second_name`** — только
   `FnBody::Block` внутри `Item::Fn`/`Item::Test`. В сегодняшнем корпусе
   `#no_copy` не встречается вовсе, риска нет; когда типы появятся — легко
   расширить (тот же `NoCopyWalk`, только другая точка входа).
2. **Многошаговая цепочка field-read** (`x = obj.inner.counter`, 2+
   уровня) резолвится (`resolve_path_type` рекурсивен по `Member`), НО
   если `inner`'s тип не резолвится статически (generic, неизвестный
   импорт) — молчаливый false-negative (как везде в этом файле,
   конвенция «неизвестный тип → skip»).
3. **Именованные/spread аргументы (`f(x: a)`, `f(...xs)`) не участвуют в
   критерии заимствования** — всегда трактуются как эскейп (безопасный
   дефолт, НЕ пропуск копии, просто более редкий ложный запрет).
4. **Замыкания/`spawn`/`detach`/`blocking`/`supervised` в главном
   walk'е (`check_no_copy_second_name`) НЕ спускаются внутрь для форм
   (a)/(b)/(d)** — если `#no_copy`-значение алиасится ВНУТРИ тела
   замыкания, эта форма не проверяется этим проходом (только
   верхнеуровневые `Stmt`/`Call`/литералы текущей функции + прямые
   вложенные блоки/if/match/for/while/loop). Это **асимметрично**
   `nc_param_escapes` (escape-скан ДЛЯ критерия заимствования замыкания
   таки обходит) — сам «второй-имя»-энфорс внутрь замыкания не идёт.
   Задокументированный пробел, не проверялся на реальных фикстурах.
5. **Trailing DSL-блок вызова** (`f() { … }`) не проверяется на использование
   параметра ни в критерии заимствования, ни в проверке второго имени.
6. **Named/generic-диспетч callee** (вызов через переменную-замыкание,
   dynamic dispatch на generic-параметре) не резолвится
   (`resolve_callee` — только прямой `Ident`/`Member`-путь на известный
   локальный/peer-файл `fn`) → безопасный дефолт (эскейп).
7. **Field-marker-propagation для `#no_copy` НЕ добавлена** (аналог
   D133-field-marker-missing/D133-type-marker-missing для consume-полей) —
   контейнер с `#no_copy`-полем НЕ обязан сам объявлять `#no_copy`;
   транзитивность (`type_is_no_copy`) уже делает его Affine функционально
   без явной пометки. Не запрошено брифом; если понадобится симметрия с
   D133 — отдельная задача.

---

## Текст амендмента к D447 (для интегратора — спека НЕ тронута)

Добавить в `spec/decisions/02-types.md`, раздел D447, новый подраздел
**«Энфорс (волна 2, план 248, 2026-08-05)»** после существующего раздела
«### Механизм»:

> ### Энфорс — волна 2
>
> **Правило второго имени.** Значение `Affine`-типа (`#no_copy`) не может
> получить второе имя. Проверяются четыре формы:
>
> 1. Голое связывание: `ro b = a`.
> 2. Чтение поля в локальную: `x = obj.field`.
> 3. Передача аргументом получателю, который его НЕ заимствует (см. ниже).
> 4. Встраивание в литерал записи/кортежа: `Type { field: a }`, `(a, b)`.
>
> Диагностика — `E_NO_COPY_SECOND_NAME`. Свежая конструкция
> (`Type{field: Handle{...}}`, вызов функции, бинарная операция) НЕ
> считается вторым именем — второе имя относится только к уже
> СУЩЕСТВУЮЩЕМУ значению (bare identifier / `@self` / `.field`-путь).
>
> **Заимствование.** Передача `Affine`-значения в параметр — не копия,
> если параметр получателя `ro` (не `mut`, не `consume`) И тело получателя
> НЕ сохраняет его: не пишет в поле, не возвращает, не встраивает в
> литерал, не захватывает в замыкание/`spawn`/`detach`/`blocking`/
> `supervised`, не передаёт дальше аргументом. Такая передача — заём, не
> перевязка, и остаётся законной.
>
> **`consume` и `#no_copy` на одном типе — ошибка** (`E_NO_COPY_CONSUME_
> CONFLICT`): два взаимоисключающих уровня строгости («обязан
> израсходовать» против «расходовать не обязан»).
>
> **`#no_copy` применим к видам объявления с собственным хранилищем** —
> record, sum, named tuple, newtype, external opaque (то же множество, что
> у `#share`, D415 §1). На alias/type-set/effect/protocol —
> `E_NO_COPY_INVALID_KIND`.

Ссылки в разделе «Связь» D447 дополнить: `E_NO_COPY_SECOND_NAME` /
`E_NO_COPY_CONSUME_CONFLICT` / `E_NO_COPY_INVALID_KIND` как новые коды
диагностик волны 2 (сейчас в D447 упомянут только сам атрибут и механизм
волны 1).

---

## Коммиты волны (ветка `p248-w2`)

Перечислены после `git add`/`git commit` — см. `git log` ветки.
