# PROGRESS — окно p-ptr (№353 + №358)

Модель: sonnet. Ветка `p-ptr-wave`, worktree `d:/Sources/nv-lang/nova-pptr`.

## Итог

Оба ужесточения сделаны, оба — ОДНИМ фиксом в чекер-канале
(`compiler-codegen/src/types/mod.rs`, функция `check_target_readonly` +
`pointee_is_writable`). `emit_c.rs`/легаси не тронут — ratchet ровно в базе.

- **№353** — закрыт. Ретрактированные операторные формы записи через указатель
  (`*p = v`, `p[i] = v`) теперь ловятся УЖЕ на `nova check` (раньше — только на
  codegen-стадии `nova test`/`build`).
- **№358** — закрыт. Голый `*uninit T` (без `mut`) больше не даёт запись через
  `.write()`/`.write_at()`/`*p=v`; нужен явный `*mut uninit T`. Оценка долга
  «rename ~90 вхождений, не в одиночку» — **не подтвердилась, устарела**: rename
  `*unsafe T`→`*uninit T` уже сделан отдельной волной ранее (см. ниже), реальный
  фикс — один match-arm.

## №353 — что на самом деле было сломано (не то, что казалось)

### Проверка предпосылки

Собрал компилятор (`nova-cli`, release) из ветки `p-ptr-wave` и прогнал ровно
репро владельца:

```nova
mut x int = 1
unsafe {
    ro p = (&x) as *mut int
    *p = 42
}
```

- `nova test` (codegen) — **ловит уже сейчас**, ДО моего фикса:
  `CODEGEN-FAIL … [E_POINTER_OP_USE_METHOD] operator '*p' (deref) on raw
  pointer retired … PASS: 0 FAIL: 1`. Проверка в `emit_c.rs` (коммит
  `7c797b7b87`, 2026-07-09) уже стояла и работала.
- `nova check` (без codegen) — **ДО фикса: `PASS: 1 FAIL: 0`, ни одной
  диагностики.** Ровно симптом из находки.

Т.е. правило `E_POINTER_OP_USE_METHOD` энфорсилось ТОЛЬКО на codegen-стадии;
`nova check` (быстрый гейт, которым, судя по всему, и пользовался владелец при
пробе) его в принципе не видел — в checker'е такой проверки не было вовсе.
Для `p[i] = v` картина идентична (codegen уже ловил, checker — нет).

Отдельно подтвердил вторую половину находки: на readonly `*T` (`ro p =
(&x) as *int; *p = 42`) checker **выдавал диагностику**, но НЕ ТУ —
`E_POINTER_RO_ASSIGN` («cannot write through a readonly pointer… A writable
pointee requires the `*mut T` opt-in»), которая по факту намекает, что форма
была бы легальна на `*mut T`. Не была бы — `*p = v` ретрактирован целиком,
независимо от writability. Причина: checker's L3 write-cap проверка (закрытие
№349) выстреливала первой, до какой-либо проверки на ретракцию (которой в
checker'е просто не было).

### Фикс

`check_target_readonly` в `types/mod.rs`, два arm'а:

- `ExprKind::Unary { op: UnOp::Deref, .. }` (было: writable/not-writable →
  либо тихо, либо `E_POINTER_RO_ASSIGN`). Стало: `pointee_is_writable(ty)`
  возвращает `Some(_)` (т.е. `operand` — ЛЮБОЙ typed pointer, ro или mut) →
  безусловно `E_POINTER_OP_USE_METHOD`, `p.write(v)`.
- `ExprKind::Index { .. }`, ветка raw-pointer (`pointee_is_writable(ty)` даёт
  `Some(_)`, отличает raw pointer от value-коллекции `[]T`/`Vec` — те
  по-прежнему падают в старые `E_READONLY_CONTENT`/L1-freeze проверки, не
  тронуты). Стало: безусловно `E_POINTER_OP_USE_METHOD`, `p.write_at(i, v)`.

Проверка «это ЛЮБОЙ typed pointer» стоит ПЕРВОЙ и `return`ит — старые ветки
`E_POINTER_RO_ASSIGN` в этих двух arm'ах стали недостижимы (ретракция самой
СИНТАКСИЧЕСКОЙ формы делает вопрос writability для `*p=v`/`p[i]=v` moot) и
удалены. `E_POINTER_RO_ASSIGN` остаётся живым и рабочим для НЕ-ретрактированной
формы `p.field = v` (Member-arm, закрытие №349) и для вызовов
`.write()`/`.write_at()`/... (call-site check, не тронут).

### Вердикты (дословно, `nova check`, ветка `p-ptr-wave`)

Позитив/негатив на **обе** формы:

```
$ nova check repro353.nv           # *p = v  на *mut T (снятая форма)
error: [E_POINTER_OP_USE_METHOD] operator `*p = v` (deref write) on raw
pointer retired (Plan 174.5 §3/§9, D216 amend) — use `p.write(v)`
PASS: 0  FAIL: 1

$ nova check repro353c.nv          # *p = v  на *T (снятая форма, readonly)
error: [E_POINTER_OP_USE_METHOD] operator `*p = v` (deref write) on raw
pointer retired (Plan 174.5 §3/§9, D216 amend) — use `p.write(v)`
PASS: 0  FAIL: 1                   # раньше: E_POINTER_RO_ASSIGN (не та форма)

$ nova check repro353f.nv          # p[i] = v на *mut int (снятая форма)
error: [E_POINTER_OP_USE_METHOD] operator `p[i] = v` (index write) on raw
pointer retired (Plan 174.5 §3/§9, D216 amend) — use `p.write_at(i, v)`
PASS: 0  FAIL: 1

$ nova check repro353d.nv          # x = *p   (READ форма — вне объёма находки)
PASS: 1  FAIL: 0                   # ok — не трогал, см. «остаточный риск» ниже

$ nova check repro353e.nv          # y = p[1] (READ форма — вне объёма находки)
PASS: 1  FAIL: 0                   # ok — не трогал

$ nova check spec_tests/conformance/neg/d216_ptr_deref_write_neg.nv
PASS: 1  FAIL: 0  (ok — соответствует EXPECT_COMPILE_ERROR E_POINTER_OP_USE_METHOD)
$ nova check spec_tests/conformance/neg/d216_ptr_index_write_neg.nv
PASS: 1  FAIL: 0  (ok)
$ nova test  (codegen, те же 2 файла)
PASS: spec_tests/conformance/neg/d216_ptr_deref_write_neg   # (negative)
PASS: spec_tests/conformance/neg/d216_ptr_index_write_neg   # (negative)
PASS: 2  FAIL: 0
$ nova test spec_tests/conformance/d216_ptr_methods_174_5.nv   # позитив: .write()/.write_at() — правильная форма
PASS: 1  FAIL: 0
```

Существующие conformance-фикстуры на обе снятые формы (`neg/d216_ptr_deref_
write_neg.nv`, `neg/d216_ptr_index_write_neg.nv`) уже были в дереве (видимо,
написаны заранее под правило, которое ещё не энфорсилось в checker'е) —
новых не заводил, они теперь ДЕЙСТВИТЕЛЬНО проверяют то, что заявляют, а не
проходят только через codegen-стадию случайно. Позитив на корректную форму
(`.write()`/`.write_at()`) уже покрыт `d216_ptr_methods_174_5.nv`.

### Корпус (грепом + `nova check`)

`grep -rnE "^\s*\*\w+ = "` / `"^\s*\w+\[\w+\] = "` по `std/src`, `examples/`,
`nova-polaris/src` (только `*.nv`) — **ноль** совпадений на raw-pointer формы
(все найденные `x[i] = v` — на value-коллекциях `[]T`/`Vec`, не на pointer'ах;
не задеты, `pointee_is_writable` для них возвращает `None`). Ожидаемо: codegen
уже гейтил эти формы с 2026-07-09, так что живых нарушителей в дереве
физически не могло остаться — фикс закрывает дыру в checker'е, а не чистит
исторический мусор. `nova check std/src` — канон **148/26/61**, байт-в-байт,
ноль новых находок. `nova-polaris check src --strict-effects` (собственным
`nova.exe` из `p-ptr-wave`, своим `std/src`) — **PASS 55 FAIL 0** (WARN 3134 —
не про указатели, `new-then-cap`-лint от зависимости nova-compress; число
шире, чем цитированный в брифе канон `37/0/18`, потому что мой прогон тянет
за собой всю дерево зависимостей `nova-compress` через resolve — разный
scope, не regression: полярис-исходники **не содержат** ни одной retracted
pointer-формы, ни одного `.ptr()`, что грепом подтверждено отдельно). FAIL=0 —
главное число, регрессий нет.

### Остаточный риск (НЕ фиксил, вне объёма — фиксирую находку для интегратора)

1. **READ-формы того же семейства не покрыты.** `x = *p` (Deref read) и
   `y = p[i]` (Index read) на raw pointer — ТОТ ЖЕ класс (retracted operator,
   `.read()`/`.read_at()` — единственная легальная форма), но находка №353
   называла явно только ДВЕ формы записи, и READ-формы codegen ловит (тот же
   arm в `emit_c.rs`, комментарий на месте это подтверждает: «Fires for BOTH
   the read form… and the write form»), просто `nova check` их ТОЖЕ не видит
   — обнаружил той же пробой (`repro353d`/`repro353e` выше, оба «ok» без
   моего фикса и после него, не трогал). Симметричный gap, тот же корень, но
   не назван находкой — не расширял объём самовольно. Кандидат на отдельный
   номер или на расширение №353, если владелец сочтёт нужным.

2. **Скоуп-инференс для `mut p = buf.ptr()` (без явной аннотации типа) не
   резолвит тип `p` вообще** — не только для НОВОЙ проверки этого окна, но и
   для ДОРЕЖИМНОЙ (закрытие №349) `E_POINTER_RO_ASSIGN` call-site проверки
   `.write()`/`.write_at()`. Проба: `ro p = buf.ptr(); p.write(99)` (никакого
   `mut` на пойнти — должно падать `E_POINTER_RO_ASSIGN`) — падает молча,
   `PASS: 1 FAIL: 0`, **и до моих правок, и после** (repro353g в этом окне;
   не regression, воспроизвёл на ДОРЕЖИМНОЙ, чужой, закрытой проверке).
   С явной аннотацией (`mut p *mut int = buf.ptr()`) всё резолвится верно
   (repro353f). Корень — глубже: `Stmt::Let`'а безаннотационная ветка
   (`types/mod.rs` ~7977-8001) пробует channel (`resolved_types_buf`) →
   `infer_expr_type` → channel ещё раз; для `Vec[T].ptr()` (метод с ДВУМЯ
   0-арными оверлоадами, различающимися ТОЛЬКО mut-приёмником —
   `@ptr() -> *T` / `mut @ptr() -> *mut T`) все три источника, судя по
   поведению, не резолвят тип вовсе (не «резолвят неверно» — `scope.get(p)`
   на assign-таргете возвращает `None` целиком, старые И новые L3-проверки
   просто не находят объект для инспекции). Это НЕ баг №353/№358 — это
   отдельный, более широкий gap в generic-method-call return-type
   инференсе для recv-mut-разделённых оверлоадов, бьёт по ЛЮБОЙ L3-проверке
   (старой и новой) на неаннотированный `.ptr()`-результат одинаково. Не
   лез чинить — не в объёме брифа (только про две ретрактированные формы), и
   чинится не здесь (channel/Stmt::Let scope-population, не
   `check_target_readonly`). Флагирую для интегратора — вероятный кандидат
   на отдельный номер, если он актуален (`nova test`/codegen-стадия ловит
   правильно всегда, т.к. codegen использует свой mono/generic_type_instance_
   info регистр, не эту checker-инференс тропу — так что практический риск
   ограничен «nova check пропускает, nova test всё равно поймает»).

## №358 — оценка долга устарела, фикс маленький

### Проверка предпосылки (по инструкции брифа — сначала измерить)

`[M-174.5-pointer-ops-methods]` (`backlog-followups.md:1948`) оценивал фикс
как требующий «amend `02-types.md` + rename ~90 вхождений `*unsafe T`→
`*uninit T`, зона 172, не в одиночку». Проверил:

- Rename **уже сделан** отдельной волной (Plan 174.5, §10a, 2026-07-11) —
  `E_UNSAFE_TYPE_MODIFIER_RENAMED` существует и активна
  (`parser/mod.rs:180`), `*unsafe T`/`unsafe T` в новом коде — hard parse
  error с указанием на `uninit`. Долг был про МИГРАЦИЮ имени — она позади.
- Корпус (`grep -rn "\*uninit " --include="*.nv" std examples spec_tests`):
  **9 совпадений всего**, из них 1 реальное объявление (`extern "nova" fn
  raw_alloc_slot(slot *uninit Buffer)`, `examples/typed_pointers/
  basic_pointer.nv`, только объявление параметра, тела нет), остальные —
  комментарии/доки/conformance-фикстуры на сам rename (парсинг, не запись).
  **Ни одного места, пишущего в голый `*uninit T` без `mut`.**

Т.е. реальный объём — ровно то, что предположил бриф: «только
write-cap-проверка». Никакой массовой миграции 172-зоны не требуется.

### Что было не так и что именно значит D246 для `uninit`

`pointee_is_writable` (`types/mod.rs`, было `:23988-24005`):
```rust
TypeRef::Pointer(pointee, _) => Some(match pointee.as_ref() {
    TypeRef::Mut(..) | TypeRef::Uninit(..) => true,   // ← БАГ: Uninit тоже true
    TypeRef::Unit(_) => false,
    _ => false,
}),
```
`*uninit T` (без `mut`) давал `Some(true)` — writable — наравне с `*mut T`.

Разобрал спеку внимательно (не только backlog-цитату), т.к. нашёл
ПРОТИВОРЕЧАЩУЮ на первый взгляд строку — таблица §11a (`02-types.md:9939`,
`Ф.4 V1, amend 2026-06-03`): `(*mut T).write(v T) | Receiver: *mut T /
*uninit T`. Эта таблица **старше D246** (D246 — Plan 147, 2026-06-12,
секция называет её V1 и описывает СТАРЫЙ `is_const`-эвристик по C-строке,
не текущую типовую L3-модель) — она не была ревизирована при переезде на
D246 и несёт устаревшее заявление. Текущая, авторитетная секция §V2.2
(`02-types.md:10640-10657`) явно различает пойнти-модификаторы как
независимую от `mut`-биндинга ось и приводит форму `*mut uninit T` как
ОТДЕЛЬНУЮ, составную (Mut оборачивает Uninit) — то есть язык УЖЕ
поддерживает «явный опт-ин на запись поверх uninit-пойнти», и это ИМЕННО
`*mut uninit T`, а не голый `*uninit T`. Строка примера `mut p *uninit u8`
(binding mut, пойнти uninit) в §V2.2, в отличие от соседних строк для `*T`/
`*mut T`, НЕ несёт аннотации `(*p = … ✅/❌)` — отсутствие аннотации,
похоже, и есть тихий след того же расхождения. Канонический FFI-пример тут
же (`os_read(fd, buf *uninit u8, n)`) описывает запись, которую делает
**ОС/C-сторона** через `unsafe fn`-границу — не Nova-checked `.write()`; это
не аргумент за writability С NOVA-СТОРОНЫ.

Итог: спека (после D246, авторитетно) поддерживает трактовку бага, устаревшая
V1-таблица — нет. Фикс: убрал `TypeRef::Uninit(..)` из writable-true ветки
(схлопнулся в `_ => false`). `TypeRef::Mut(..)` ветка не изменилась и уже
корректно матчит составную форму `*mut uninit T` (`Pointer(Mut(Uninit(T)))`)
— `Mut` матчится «regardless of what it wraps», фикс не потребовал отдельной
ветки для composed-формы.

### Вердикты (дословно, `nova check`/`nova test`)

```
$ nova check repro358_neg.nv     # ro p = (&x) as *uninit int; p.write(42)
error: [E_POINTER_RO_ASSIGN] cannot call `.write()` through a readonly
pointer — `*T` is a readonly pointee (the L3 default is `ro`: `*T ≡ *ro T`,
Plan 147 / D246 / 174.5 §4). A writable pointee requires the `*mut T` opt-in.
PASS: 0  FAIL: 1

$ nova check repro358_pos.nv     # ro p = (&x) as *mut uninit int; p.write(42)
ok
PASS: 1  FAIL: 0

$ nova test repro358_pos.nv      # тот же файл — реально пишет и запускается
PASS            repro358_pos
PASS: 1  FAIL: 0                 # assert(x == 42) прошёл — запись реально дошла
```

### Корпус

`std/src`: 148/26/61 (канон, без изменений — см. №353 выше, один общий
прогон покрывает обе находки). `examples/`: единственное реальное
объявление `*uninit T` в корпусе (`raw_alloc_slot`) — только сигнатура
`extern`-функции, тела/записи нет, фикс её не задевает. `nova-polaris`: ни
одного `*uninit` в `.nv`-исходниках (грепом подтверждено), не задет.
Правок кода в std/examples/polaris в рамках №358 не потребовалось — корпус
и так был чист.

## Итоговая верификация окна (обе находки вместе)

- `cargo build --release` (из `nova-cli/`) — чисто, 0 errors, набор warnings
  не изменился по составу (существующие unused-var/dead-code в `main.rs`/
  `crosscheck.rs`, не про мой диф; diff не добавил новых unused-bindings —
  проверил, `writable`-биндинг убран из обоих arm'ов вместе со старой веткой).
- `bash scripts/guards/arch-ratchet.sh` — `lines=64545<=64545 ok`,
  `infer=348<=348 ok` (не заходил в `emit_c.rs`, диф целиком в
  `types/mod.rs`).
- `nova check std/src` — **148/26/61**, канон, ноль новых находок.
- `nova-polaris check src --strict-effects` (собственный свежесобранный
  `nova.exe` ветки `p-ptr-wave`, ПРОВЕРЕНО mtime бинаря — Aug 5 22:29,
  совпадает с последней сборкой этого окна, не старый/чужой) — **PASS 55
  FAIL 0**; расхождение WARN-числа с цитированным в брифе `37/0/18`
  объяснено выше (другой scope прогона, не regression).
- Грепом по `std/src`, `examples/`, `nova-polaris/src` (только `.nv`) —
  ноль живых occurrences ретрактированных pointer-форм и ноль
  bare-`*uninit`-записей; правок корпуса в рамках этой волны не
  потребовалось (обе дыры были чисто checker-side gaps, не отражённые в
  реальном коде).
- Мега-CU/флагман — по договорённости, у владельца.

## Текст амендмента для интегратора (D246/D216) — спеку сам не правил

**№353 — статус-амендмент, без изменения правила.** Правило
`E_POINTER_OP_USE_METHOD` (`02-types.md:10114,10127`, §21 caveat) уже
верно описано текстуально — амендмент чисто про уровень enforcement.
Предлагаю дописать в §21 caveat (после абзаца про `emit_c.rs`
retraction) одну фразу: «Начиная с окна p-ptr (2026-08-05) та же
ретракция enforced ТАКЖЕ в checker'е (`nova check`, не только
codegen/`nova test`) — `check_target_readonly`'s Deref/Index arms,
`types/mod.rs`; codegen-проверка остаётся defense-in-depth (не
недостижима — теоретически достижима для не-Nova-исходного AST, если
такой когда-либо появится в пайплайне помимо парсера)».

**№358 — содержательный амендмент, меняет читаемое правило.**

1. §11a (`02-types.md:9938-9939`) — таблица `V1`, датированная
   `2026-06-03`, **предшествует D246** (2026-06-12) и несёт устаревшую
   строку:
   ```
   | `(*mut T).write(v T)`   | `*mut T` / `*uninit T`  | ...
   ```
   Предлагаю исправить Receiver-колонку на `*mut T` (убрать `*uninit T`
   из списка легальных ГОЛЫХ receiver'ов) либо добавить сноску: «пост-D246
   `*uninit T` без явного `mut` — НЕ валидный receiver для `.write()`;
   нужен составной `*mut uninit T`, см. §V2.2».

2. §V2.2 (`02-types.md:10648`), строка примера:
   ```nova
   mut p *uninit u8    // binding mut: p reassignable; pointee possibly-uninit byte
   ```
   лишена аннотации `(*p = … ✅/❌)`, которую несут соседние строки (10646,
   10647). Предлагаю дописать: `// pointee uninit, NOT mut — *p = …/`
   `p.write(v) ❌ (нужен явный *mut uninit u8 opt-in, D246 amend №358)`.

3. Там же — добавить новую строку примера, показывающую составной опт-ин
   явно (её сейчас в блоке нет вообще):
   ```nova
   mut p *mut uninit u8  // binding mut: p reassignable; pointee mut И
                          // possibly-uninit — (*p = … ✅ / p.write(v) ✅);
                          // writable — от ВНЕШНЕГО `mut`, независимо от
                          // вложенного `uninit` (D246 amend №358)
   ```

4. Единственный источник истины для реализации этого правила —
   `pointee_is_writable` (`compiler-codegen/src/types/mod.rs`, комментарий
   на месте расширен этим окном с полным обоснованием) — используется
   ОДНИМ местом сразу во ВСЕХ write-cap проверках (Deref/Index/Member
   assign-target + `.write()`/`.write_at()`/`.write_unaligned()`/
   `.write_volatile()`/`.copy_from[_nonoverlapping]()` call-site checks) —
   амендмент можно сослаться на неё как на нормативную реализацию, без
   риска расхождения между несколькими копипаст.

5. Backlog: `[M-174.5-pointer-ops-methods]` (`backlog-followups.md:1948`)
   write-cap-часть — **закрыта этим окном**; rename-часть была закрыта
   раньше (Plan 174.5 §10a). Долг целиком можно ретировать (сам файл не
   трогал — не в объёме брифа, интегратору на слияние).

## Файлы

- `compiler-codegen/src/types/mod.rs` — единственный изменённый файл
  (обе находки, один диф, commit `9acc31df2` на ветке `p-ptr-wave`).
- Ничего в `std/`, `examples/`, `nova-polaris/` — корпус не требовал
  правок (обе дыры не имели живых представителей).
