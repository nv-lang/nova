# REPORT-p971 — необъявленное имя типа в аннотации (реестр 221.1 №971)

Ветка: `p971-undeclared-type-name-yltj2o` (база — `origin/integrate` `9712b306f`; к сдаче
`origin/integrate` новых коммитов не имел, слияние — пустое).

**ВОСПРОИЗВЕДЕНИЕ:** проба `docs/plans/repro/971-undeclared-type-name-accepted/` на базе
(`nova check`, std через `NOVA_STD_PATH`): `unknown_param_runs`, `unknown_return_runs`,
`typo_generic_runs`, `float_field_loud`, `extern_sig_unimported_type` — PASS; оба контроля —
`E7301`. Фикстуры Карины `novac/fixtures/undeclared_type/neg_1..4.nv` — оракул PASS на всех
четырёх.

**КОРЕНЬ:** `compiler-codegen/src/types/mod.rs`, `walk_typeref`, ветка `TypeRef::Named`:
`// Неизвестное имя — не наша забота (name-resolution).` + `let Some(info) = self.arity.get(name)
else { return; }`. Имя без записи в arity-таблице молча выходило из проверки, становилось
непрозрачным номинальным типом, и все проверки в этой позиции выключались.

**ФИКС:** `compiler-codegen/src/types/mod.rs:8463` — на месте молчаливого `return` дверь
`E_UNKNOWN_TYPE` (та же, что у литерала записи, план 173 Ф.5). Новый код — дочерний модуль
`compiler-codegen/src/types/unknown_type_name.rs`:
- `annotation_type_name_visible` (:58) — готовый четырёхчастный тест литерала
  (`Self`, `gs`, `types_get_here_contains`, `sum_variant_names`) плюс то, что аннотация может
  назвать, а литерал нет: `arity_exempt`, примитив, протокол-алиас, `ChanReader`/`ChanWriter` (D91);
- `add_bound_introduced_vars` (:74) — имена из АРГУМЕНТОВ баундов кладутся в `gs` (D355 §1),
  вызывается для `fd.generics`, `carrier_bounds` приёмника, `td.generics`, дженериков методов
  эффекта и протокола (`mod.rs:7496,7498,7560,7598,7616`);
- `walk_effect_ref` (:104) — запись строки эффектов: голова (`Io`, `Net`…) не судится, аргументы
  (`Fail[X]`) судятся;
- `unknown_annotation_type_diag` (:126) — текст: «unknown type `X`: no type of this name is in
  scope here -- it is neither declared in this module nor imported (a misspelling, or a type from
  a module this file does not import)»; для `usize`/`isize` — прежняя фраза эмиттера «is removed —
  use `int` (Plan 133)».

Обход — существующий `walk_typeref` (46 площадок); новый проход по декларациям НЕ заведён.

**КЛЕТКИ** (позиция × форма `TypeRef` -> до / после; каждая клетка — негатив
`spec_tests/conformance/neg/p971_undeclared_type_<ключ>_neg.nv`):

| позиция / форма | ключ | до | после |
|---|---|---|---|
| возврат, `Named` | `return` | PASS | `E_UNKNOWN_TYPE` |
| параметр, `Named` с аргументами (`Vecc[int]`) | `param` | PASS | `E_UNKNOWN_TYPE` |
| поле записи, `Named` | `field` | PASS | `E_UNKNOWN_TYPE` |
| локал `ro x Nmbr = 1` | `local` | PASS | `E_UNKNOWN_TYPE` |
| аргумент дженерика `Vec[Nmbr]` | `generic_arg` | PASS | `E_UNKNOWN_TYPE` |
| `Array` `[]X` | `slice` | PASS | `E_UNKNOWN_TYPE` |
| `FixedArray` `[4]X` | `fixed_array` | PASS | `E_UNKNOWN_TYPE` |
| `Tuple` `(X, int)` | `tuple` | PASS | `E_UNKNOWN_TYPE` |
| `Func` `fn(X) -> int` | `fn_type` | PASS | `E_UNKNOWN_TYPE` |
| `Readonly` `ro X` (возврат `extern "nova"`) | `readonly` | PASS | `E_UNKNOWN_TYPE` |
| `Pointer` `*X` | `pointer` | PASS | `E_UNKNOWN_TYPE` |
| payload варианта суммы | `variant_payload` | PASS | `E_UNKNOWN_TYPE` |
| цель алиаса | `alias` | PASS | `E_UNKNOWN_TYPE` |
| сигнатура `extern "C"` | `extern_sig` | PASS | `E_UNKNOWN_TYPE` |
| аргумент эффекта `Fail[X]` | `effect_arg` | PASS | `E_UNKNOWN_TYPE` |

`?X` в языке нет (парсер: «expected type, got `?`») — клетка не применима, `Option[X]` покрыт
`generic_arg`. `Mut`/`Uninit`/`Protocol`/`Unit` идут через ту же рекурсию `walk_typeref`;
отдельной фикстуры нет. Голова строки эффектов — НЕ судится (см. ОТКРЫТО).

**ПРИМИТИВЫ:** единым стал `PRIMITIVE_TYPE_NAMES` (`unknown_type_name.rs:33`, 15 имён:
`int i8 i16 i32 i64 u8 u16 u32 u64 uint f32 f64 bool char str`). Сведены ЧЕТЫРЕ копии (строка
реестра знала три): `is_primitive_type_name` (15) — теперь читает const; затравка arity-таблицы в
`TypeCheckCtx::build` (15) — читает const; `is_primitive_scalar_type_name` (14, БЕЗ `uint`) —
удалена, её площадка `E_RECV_GENERIC_SHADOWS_TYPE` зовёт `is_primitive_type_name`, то есть
`uint` в слоте приёмника теперь тоже тень, как остальные примитивы (доктрина №88 (iv) банит
примитивы — пропуск `uint` был недосмотром); локальный `primitives` баунд-проверки (17, с
`any`/`never`) — удалён. `any`/`never` НЕ примитивы: это top/bottom из `arity_exempt`, поэтому в
единый список не вошли; баунд-проверка `check_generic_bound_declarations` называет их явно
(`|| matches!(name, "any" | "never")`). Расхождение объяснено и снято.

**ЦЕНА:** замер новым бинарём против базового, вердикты по файлам сверены `diff`.
- черновик 2026-09-06 (из реестра): 6539 срабатываний на `std/src`;
- после снятия ложняков (баунд-имена D355, алиасы, D91-типы) — **32** срабатывания
  аннотационной двери на всех корпусах, все классифицированы:
  - `std/src` — **25, все НАСТОЯЩИЕ**: 24 = `Duration` в восьми `extern "nova"`-сигнатурах
    `std/src/runtime/sync.nv` (:1659, 1771, 1801, 1893, 2274, 2380, 2495, 2608; каждая
    считается дважды — как файл и в составе папки) — добавлен `import std.time.duration.{Duration}`;
    1 = `CallerLoc` в `std/src/prelude/runtime.nv:118` (`caller_loc() -> ro CallerLoc`, тип в
    `prelude/core.nv:44`) — добавлен `import std.prelude.core.{CallerLoc}` (так же импортируют
    `Option` соседние файлы prelude). После: 0; вердикты 184 файлов байт-в-байт как на базе
    (PASS 158 / FAIL 26 у обоих);
  - `examples` — **2, НАСТОЯЩИЕ**: `Timestamp` в `examples/real_world/orm_demo.nv:56,64` —
    добавлен `import std.time.duration.{Timestamp}`. Файл красный и на базе по другим причинам
    (`E_BOUND_NOT_SATISFIED`, `E_NO_STATIC_METHOD` на `Timestamp.from_unix`); вердикты 104 файлов
    как на базе;
  - `spec_tests/conformance` — 1 ложняк (`ChanReader` в
    `m_ice_channel_reader_try_recv_binding_pos.nv`, D91-тип без `.nv`-объявления — снят списком
    `LANGUAGE_INTRINSIC_TYPES`) и 3 законных отказа: негативы `neg_int_abs_removed`,
    `t1_usize_removed_neg`, `t2_isize_removed_neg` (`usize`/`isize`) — на `check` теперь FAIL
    (раньше их ловил только эмиттер), в `nova test` все три зелёные (фраза эмиттера сохранена);
  - `novac` — **1, НАСТОЯЩИЙ, НЕ исправлен**: `novac/src/emit_c/shell.nv:121`, поле
    `fres FieldIndex` — тип в `novac/src/resolve/resolve.nv:183`, в модуле `novac.emit_c` не
    импортирован. Папка `novac/src/emit_c` теперь FAIL на `nova check novac` (на базе ok).
    Самосборка Карины НЕ ломается (в CU `main.nv` имя видно). Файл — чужой волны, вопрос 1 ниже;
  - `spec_tests/soundness`, `spec_tests/strict_effects`, `spec_tests/p270`,
    `nova_tests/contracts` — 0, вердикты как на базе.
- **Отказ включён: да** — непроклассифицированных срабатываний нет, настоящих носителей 28
  (25 + 2 + 1), все — пропущенный `import`.

**КАРИНА:** строк `novac/divergences.allow` **8 -> 4** (сняты
`novac/fixtures/undeclared_type/neg_1..neg_4.nv`); `docs/plans/274.12-novac-divergences.md` —
счёт в зеркале «ВОСЕМЬ» -> «ЧЕТЫРЕ», раздел №971/№970 помечен «УШЛО 2026-10-01 (№971, ветка
`p971-undeclared-type-name-yltj2o`)». Исход на `neg_1..4`: оракул — FAIL у всех четырёх
(`E_UNKNOWN_TYPE` на `Strng`, `Vecc`, `Nmbr`, `Nmbr`); Карина (собрана этим оракулом из
`novac/src/main.nv`) — rc=1 у всех четырёх (`E_NOVAC_SUBSET` «undeclared type name»). Оба
отвергли — расхождения исхода нет.

**ФИКСТУРЫ:** 15 негативов `spec_tests/conformance/neg/p971_undeclared_type_*_neg.nv` (по одной
ошибке, построчный `// nova:expect E_UNKNOWN_TYPE -- <причина>`), позитив
`spec_tests/conformance/standalone/p971_visible_type_names_pos.nv` (`EXPECT_STDOUT`): prelude
без импорта (`Vec`, `Option`, `Result`, `str`), импортированный `Duration`, параметр-дженерик,
имя из баунда (`fn[I Next[T]] I mut @p971_sum_with`, `T` в аннотации локала тела), `ChanReader`,
все 15 примитивов. Проба в обе стороны на одном дереве: дверь выключена (`if false &&`) — все
15 негативов NEG-NO-ERROR, позитив PASS; дверь возвращена — `--filter p971_` PASS 16 / FAIL 0.
Выключатель удалён, флага `NOVA_*` нет.
Соседи (`nova test spec_tests --full --filter …`): `p1234` 5/0, `p1444` 6/0, `import` 16/0,
`generic` 56/0, `extern` 9/0, `alias` 20/0, `blanket` 13/0, `chan` 11/0, `removed` 11/0,
`record_lit` 2/0, `d192` 1/0; `unknown_type`, `d355` — 0 тестов под фильтром; `bound` 28/1 —
единственный красный `soundness/ovf_unbounded_panic_neg` (`E_D78_MODULE_PATH_MISMATCH`) красный и
на базе тем же текстом. `nova test std/src/collections std/src/text std/src/io` — 18/0 (и
`--full` 21/0). Флагманы (`scripts/guards/flagship-targets.txt`): aggregator, http_proxy_chain,
echo_server_net, echo_client_tls, echo_server_tls — собираются; `echo_client_net` — НЕ
собирается, тем же C-отказом и на базовом бинаре (`passing 'const nova_str' to parameter of
incompatible type 'Nova_Vec____nova_byte *'`, `echo_client.c:11142`) — не от этой правки.
Самосборка Карины `nova build novac/src/main.nv` — ok.

**КРЕЙТ:** `compiler-codegen` `RUST_MIN_STACK=134217728 cargo test --lib` — прошло,
1278 passed / 0 failed.

**ХРАПОВИК:** `emit_c.rs` строк 67690 (не тронут; `arch-ratchet ok: lines=67690 <= 68857`).
Стражи: `check-registry-single-verdict` ok, `check-registry-routes` ok (блокеров 95 при базе 95:
№971 ушёл из открытых, новая строка №TBD пришла), `check-nova-expect-ratchet` ok (554 <= 554),
`check-test-fixture-coverage` ok, `check-commit-language` ok. `check-invariant-discipline` красный
на `compiler-codegen/nova_rt/channels.h` — файл этой веткой не тронут, красный на базе.

**КОММИТЫ:** `5f917d224` types: an undeclared type name in an annotation is refused (#971) —
фикс, импорты std/examples, фикстуры, реестр, allow, 274.12; следующий — этот отчёт.

**ОТКРЫТО:**
1. `novac/src/emit_c/shell.nv:121` — настоящий носитель (`FieldIndex` без импорта), не исправлен
   по запрету трогать `novac/` вне allow/274.12 (вопрос 1).
2. Голова строки эффектов не судится: `fn f() Ioo -> int => 1` — `check` PASS и на базе, и после
   фикса. Заведена строка `| №TBD |` (🟡 К2, БЛОКИРУЕТ ТЕГ: ДА, МАРШРУТ: канал 196) — нужен один
   источник «встроенные эффекты, видимые без импорта»; сейчас он есть только в линте
   (`lints.rs::collect_effect_names`).
3. Видимость CU-широкая, не пофайловая: `types_get_here` пофайлов только для коллидирующих
   имён, поэтому тип, объявленный в модуле, который CU втянул по чужому импорту, проходит и в
   файле без импорта. Это свойство готового теста литерала (взят по заданию), не новое.
4. Карина на `undeclared_type/neg_4.nv` называет `Vec`, а не `Nmbr` («undeclared type name
   `Vec`») — похоже, одиночный `novac check` не получает prelude; исход совпадает, текст — дело
   окна Карины. Комментарии в `novac/fixtures/undeclared_type/neg_*.nv` ещё говорят «the oracle
   accepts it» — не тронуты (`novac/` вне разрешённых файлов).
5. Из проб `docs/plans/repro/971-…`: `float` в поле теперь ловится чекером
   (`E_UNKNOWN_TYPE` на `float`), а не Си. Расхождение прозы спеки («int и float») с именами
   типов (реестр, пункт «ПОБОЧНОЕ») — не тронуто.

**ВОПРОСЫ ИНТЕГРАТОРУ:**
1. **`novac/src/emit_c/shell.nv:121` — `FieldIndex` без импорта.** Решить: кто и когда добавляет
   строку `import ../resolve.{FieldIndex}` в `novac/src/emit_c/shell.nv`. Варианты: (а) интегратор
   при слиянии этой ветки одной строкой; (б) отдать окну, ведущему `emit_c` Карины (сейчас идёт
   №1461); (в) не сливать, пока импорт не придёт. **Рекомендация — (а):** правка механическая,
   той же формы, что три импорта этой ветки в std/examples; без неё `nova check novac` даёт
   новый FAIL на папке `emit_c` (самосборка при этом не ломается — имя видно в CU `main.nv`).
   Не сделал сам только из-за запрета на файлы `novac/` в задании.
2. **Имена, введённые баундом, вне blanket-формы D355.** Я ввожу их из аргументов ЛЮБОГО баунда
   (`fn[T Test[K]]`, `type X[T P[U]]`, методы протокола/эффекта), а не только у ресивера
   `fn[I Proto[T]] I @m`. D355 §1 говорит про blanket-форму; D72 «K не объявлен вообще — ОШИБКА»
   стоит в разделе, СНЯТОМ 2026-08-16 (№702), и прямого слова о небланкетном случае нет. Решить:
   (а) оставить так (статус-кво: до фикса эти имена тоже проходили, новых отказов нет); (б)
   сузить до ресивера blanket-формы и carrier-баундов — тогда `fn f[T Test[K]](v K)` получит
   `E_UNKNOWN_TYPE` на `K`. **Рекомендация — (а) до слова спеки:** (б) — изменение языка, нужен
   D-блок; замера цены (б) я не делал.
