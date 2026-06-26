<!-- SPDX-License-Identifier: CC-BY-4.0 -->
# Plan 181 — Единый fallible-контракт std: **Result-everywhere** (no bare-throws convention)

> **Top-level план.** Создан 2026-06-25. **Статус:** 📋 PROPOSED (требует sign-off нейминг-правила §2 + governance-разрешения на §5 Ф.1).
> **Маркер:** `[M-181-result-everywhere-std]`. **Запуск:** «**выполни план 181**».
> **Решение (2026-06-25):** **Вариант 1** — **вся публичная std возвращает `Result[T, E]`** на любой падающей операции.
> Дуальный `bare`(throw)/`try_`(Result)/`_opt`(Option)-нейминг **РЕТРАКТИРУЕТСЯ** из std. Эффект `Fail[E]` **остаётся в языке**
> (для пользовательского кода и внутренних хелперов), но публичный std-API его для своих ошибок не несёт. throw на call-site = `!!`, проброс = `?`, ветвление = `match`, `Result→Option` = `.ok()`.
> **D-блок (NEW):** **D325** (`spec/decisions/04-effects.md`) — единое правило; **amends/retracts D77** (4-way auto-derive bare-формы) и **D178** (`parse_int` bare + `parse_int_opt`).
> ⚠ **D316–D324** зарезервированы планами 179/179.1/180 → берём **D325**, gap фиксируем в индексе.
> **Эталон (живой код):** [std/net](../../std/net/effect.nv) — уже Result-everywhere, 0 `Fail[`. Под Вариантом 1 это **просто норма**, а не «исключение».
> **Координация:** **Plan 171** (primitive parse — выровнять под Result, §10), Plan 172.3 (type-set bounds — ортогонально), Plan 180 (io/fs/os — уже Result), Plan 173 (error-MACHINERY — нейминга не касается).
> **Решение принято осознанно** на этапе до-прода: «спроектировать правильно сейчас, переделать сделанное, если нужно» — причина объективная (см. §1), не sunk-cost.

---

## 0. TL;DR

1. **Одно правило на язык для std:** любая падающая публичная операция → **`Result[T, E]`**. Без bare-throws-близнецов, без `try_`-дублей, без `_opt`.
2. **Эффект `Fail[E]` не удаляется** — он остаётся механизмом языка. Просто std им свои ошибки наружу не отдаёт. Хочешь throw-стиль в **своём** коде — пиши `Fail[E] -> T`, язык позволяет.
3. **Эргономика throw сохранена операторами:** `expr!!` (throw из Result), `expr?` (проброс), `expr.ok()` (→Option), `match`.
4. **Нейминг (§2):** обычное имя = Result-форма (`parse_int -> Result`). Префикс `try_` — **только** чтобы отличить fallible-вариант от одноимённого **infallible** (`from`/`try_from`). `Option` — только для genuine absence (`find`/`get`/`env`), не для fallibility.
5. **Ретракции:** D77 4-way (bare-форма конверсий), D178 (`parse_int` bare + `parse_int_opt`), nv-coding-style §4 дуал-булет, само понятие «двух категорий».
6. **Миграция:** `parse.nv` (`try_parse_int`→`parse_int`, удалить bare+`_opt`), `read_buffer.nv` (`try_read_X`→`read_X`, удалить 24 bare-twin), `emit_c.rs` builtins, call-sites. net/Plan 180 — без изменений. `_experimental` — отложить с TODO (§9 Q3).

---

## 1. Контекст — почему Вариант 1 (объективно, не sunk-cost)

Развилка прошла три шага: **A** (bare=throws everywhere) → **B1** (две категории: I/O=Result, scalar=дуал) → **Вариант 1** (всё Result). A отвергнут (close-footgun на must-consume); B1 отвергнут как «слишком сложно для рядового разработчика + вечная граница». Объективные причины за Вариант 1 (на годы, не «жалко переделывать»):

1. **`Result` безопасен в 100% операций; bare-throws — нет.** Мы сами запретили bare-throws для файлов (`close` глотает ошибку → потеря данных, [180:29-32](180-io-fs-os.md)). Примитив, которому нужна **граница**, чтобы быть безопасным, слабее универсального.
2. **Нет границы — нет вечного налога.** Две категории = бесконечная серия «а это куда?» (snowflake был первым). Одно правило → вопроса не существует; компилятор не должен охранять рубеж, которого нет.
3. **Ошибка-как-значение фундаментальнее, чем как-throw.** `Result` кладётся в `Vec`, мапится, собирается (`[]Result` при построчном разборе), возвращается из замыкания, шлётся в канал. Брошенный `Fail` — это control-flow, как данные не используется. Где нужна «ошибка как данные» — всё равно нужен `Result`. Значит `Result` — то, без чего не обойтись; bare-throws — необязательная надстройка.
4. **Меньше имён на операцию.** Сейчас один разбор = до трёх имён (`parse_int`/`try_parse_int`/`parse_int_opt`). Одно имя + операторы — меньше поверхности, доков, путаницы «какой звать».
5. **`!!` уже даёт throw, когда нужен.** Реальная потеря — 2 символа на проброс в скриптах, и только там.

> **Сознательный trade-off:** теряем operator-free краткость в glue-скриптах (`read_file(p)` vs `read_file(p)!!`). Принято: единообразие и композируемость std важнее на дистанции десятилетий. Эффект `Fail` остаётся в языке → скрипт-стиль доступен в пользовательском коде, просто не навязан std.

---

## 2. 🎯 ЯДРО — единое правило нейминга

**(R1) Любая падающая публичная операция std → `Result[T, <Domain>Error]`.** Один структурный `XError` на домен.

**(R2) Имя — обычное, без маркера-префикса.** `str.parse_int(s) -> Result[int, ParseIntError]`, `rb.read_u32() -> Result[u32, ReadBufferError]`, `File.open(p) -> Result[File, IoError]`. (Как Rust `str::parse -> Result`.)

**(R3) Префикс `try_` — ТОЛЬКО для дизамбигуации fallible-варианта одноимённого infallible.** Существует `from` (infallible, D73) → fallible-вариант = `try_from` (Result, D77). `into` → `try_into`. Здесь `try_` маркирует **«может упасть»** относительно чистого `from`, а не «Result-близнец bare-формы». В одиночных fallible-операциях (нет infallible-сиблинга) префикса НЕТ.

**(R4) `Option` — только для genuine absence, не для fallibility.** `find`/`get`/`env`/`parent`/`Metadata.modified` → `Option` (отсутствие ≠ ошибка). `Result → Option` через `.ok()`. **Никаких `_opt`-имён.**

**(R5) Эффект `Fail[E]` в публичном std-API — запрещён для СОБСТВЕННЫХ ошибок, но разрешён для прозрачного проброса пользовательского.** Higher-order-функция, прокидывающая `Fail[E]` из closure-параметра, эффект-полиморфна и легальна:
```nova
// ЛЕГАЛЬНО: Fail[E] forwarded из тела пользователя (не своя ошибка std):
fn retry[T, E](body fn() Fail[E] -> T, policy RetryPolicy) Time Random Fail[E] -> T
// НЕЛЕГАЛЬНО под R1: своя ошибка std через throw:
fn Db.query(q Sql) Db Fail[DbError] -> []DbRow      // → Result[[]DbRow, DbError]
```
Дискриминатор для guard'а (§8): несёт ли сигнатура `Fail[E]`, **происходящий из `fn() … Fail[E]`-параметра** (forwarded — ок), или это собственная ошибка функции (→ обязан Result).

**Эталон:** [std/net](../../std/net/tcp.nv) — Result-everywhere, 0 `Fail[`. **Под-паттерны (conformant):** per-element `DirIter.next -> Result[Item, E]`; absence → `Option`; инфаллибл-аксессоры → чистое значение.

> Граница «I/O vs scalar» из B1 **упразднена** — её больше нет ни в правиле, ни в коде, ни в голове разработчика.

---

## 3. Ретракции (что откатываем)

| Решение | Было | Под Вариантом 1 |
|---|---|---|
| **D77** (TryFrom/TryInto) | 4-way auto-derive: из `try_from`(Result) компилятор генерит bare `from`(throws) | **Amend:** убрать авто-генерацию bare-throws fallible-формы. Остаются `from`(infallible) + `try_from`(Result). «4-way» → «2-way». |
| **D178** (str.parse_int) | `parse_int`(bare throws) + `try_parse_int`(Result) + `parse_int_opt`(Option) | **Retract bare + `_opt`:** одна форма `parse_int -> Result`; Option через `.ok()`. |
| **nv-coding-style §4** дуал-булет (83-91) | «дуал bare/try_ — общая конвенция» | **Retract:** заменить на R1-R5 (единое Result-правило). |
| **nv-coding-style §4** net-carve-out (92-94) | «net — открытый вопрос, Plan 173 унифицирует» | **Delete:** под Вариантом 1 net — просто норма; carve-out не нужен. |
| **«две категории» (бывш. B1)** | Cat 1 / Cat 2 + граница | **Удалить концепцию целиком.** |
| **D25** (`Fail[E]` throw) | механизм throw/Fail | **Без изменений** — остаётся в языке; меняется только std-конвенция (не использовать наружу). |
| **D85** (`?`/`!!`/`??`) | операторы | **Без изменений** — несущая эргономика Варианта 1. |

---

## 4. D325 — текст решения (draft для `04-effects.md`, после D85)

```
## D325 — Единый fallible-контракт: публичный std возвращает Result

Статус: проектное решение (Plan 181, 2026-06-25). Amends D77 (убрать bare auto-derive),
retracts D178 bare/_opt. Cross-link: D25 (Fail остаётся в языке), D30 (нейминг), D73 (From/Into),
D77 (TryFrom), D85 (?/!!/??).

(R1) Любая падающая ПУБЛИЧНАЯ операция std возвращает Result[T, <Domain>Error]. Один
     структурный XError на домен. Нет bare-throws-близнецов, нет try_-дублей, нет _opt.
(R2) Имя обычное, без префикса: parse_int -> Result, read_u32 -> Result, open -> Result.
(R3) Префикс try_ — только чтобы отличить fallible-вариант одноимённого INFALLIBLE
     (from/try_from, into/try_into). Иначе префикса нет.
(R4) Option — только genuine absence (find/get/env/parent), НЕ fallibility. Result->Option = .ok().
(R5) Эффект Fail[E] в публичной std-сигнатуре запрещён для СОБСТВЕННЫХ ошибок (→ Result),
     но разрешён для прозрачного проброса Fail[E] из closure-параметра (effect-polymorphic
     forwarding, напр. retry/parallel/in_transaction над телом пользователя).

Эффект Fail[E] (D25) ОСТАЁТСЯ механизмом языка — для пользовательского кода и внутренних
хелперов. Меняется только std-конвенция: std не отдаёт свои ошибки через throw.
Эргономика throw на call-site сохранена операторами (D85): expr!! (throw), expr? (проброс),
expr.ok() (->Option), match (ветвление).

Эталон: std/net (Result-everywhere, 0 Fail[). Под-паттерны: per-element -> Result[Item,E]
(DirIter.next); absence -> Option; инфаллибл-аксессор -> значение.
```

> **Гигиена D-нумерации:** D316–D324 зарезервированы планами 179/179.1/180 (в `spec/decisions/` не внесены, committed до D315). Берём **D325**; reserved-gap отметить в `spec/decisions/README.md`.

---

## 5. Правки конвенций (diff-ы — Ф.1, после sign-off; governance §9 Q2)

> 🔒 Конвенции нормативны. Каждая правка → дата-запись в [conventions-governance.md](../conventions-governance.md).

- **(E1) `nv-coding-style.md` §4 строки 83-91** — заменить дуал-булет на R1-R5 (единое Result-правило; `try_` только для from/try_from; Option=absence; `Fail` остаётся в языке для своего кода).
- **(E2) `nv-coding-style.md` §4 строки 92-94** — **удалить** net-carve-out (нет «открытого вопроса / Plan 173»); net — просто пример нормы.
- **(E3) `nv-coding-style.md` §20.4 строки 634-640** — пример `read_config` в Result-форму:
  ```nova
  fn read_config(path str) Fs -> Result[Config, IoError] {
      consume file = Fs.open(path)? {        // ? разворачивает Result; File must-consume → consume-scope
          ro raw = file.read_all()?
          Config.parse(raw)                  // close-Result сворачивается on_exit'ом (ENOSPC виден)
      }
  }
  ```
- **(E4) `module-conventions.md` §3 строка 92** — «Парный bare+try_ — канон» → «**Все fallible-операции → `Result[T, XError]`** (R1, D325); эффект `Fail` наружу не отдаём». §2 (74-81, `@close()->Result`) — **без правки** (уже верно).
- **(E5) `module-conventions.md` §5 строки 152-153** — `try_`-дуал убрать; «fallible → Result, без bare-twin». `from`/`try_from` (infallible/fallible) оставить.
- **(E6) `idioms/error-handling.md` 42-47, `strings.md` 368-371** — переписать «bare/try_/_opt»-конвенцию на единое Result-правило + `.ok()` для Option.
- **(E7) `std/prelude/protocols.nv` 126-138** — текст конвенции: убрать «bare from auto-derived via D77 4-way»; оставить `try_from`/`try_into` (Result) + `from`/`into` (infallible).
- **(E8) `plans/173-*.md`** — снять устаревший claim «Plan 173 унифицирует net-нейминг».
- **(E9) `conventions-governance.md`** — дата-запись 2026-06-25: «Вариант 1 / D325; ретракт дуала из std; правки §4/§20.4 + module-conventions + protocols.nv».

---

## 6. Миграция std — разбита на **.nv-only (сейчас)** и **compiler-gated (отложено)**

> 🔑 Решение владельца: правки **только в `.nv`** — можно сейчас (с подтверждением каждой); всё, что трогает **компилятор** (`emit_c.rs`) — откладываем. Discovery-workflow (`discover-v1-nv-only-migration`, 2026-06-25) классифицировал каждый пункт.

### 🟢 Ф.2a — `.nv`-only, можно сейчас (без компилятора)

| Файл | Действие | Размер |
|---|---|---|
| `std/encoding/base64.nv` ✅ **DONE (исходник, 2026-06-25)** | `Base64.decode`/`decode_url` → `Result[[]u8, Base64Error]`; `decode_with` → Result; `decode_or_throw`→`decode_at` (Result) + `?`; `throw`→`Err`/`return Err`; тесты :339-359 (`!!` для success, `Err(...)`-match для neg). `nova check` ✅. **⚠ полный `nova test` блокирован 2 пре-существующими codegen-багами (см. ниже)** — не от миграции. | **малый, самый чистый** |
| `std/_experimental/math/complex.nv` ✅ **DONE 2026-06-26** (`a2d01a67`, ветка plan-172) | Миграция re-applied после codegen-фикса (`Complex.from(s str)`→`Complex.try_from(s) -> Result[Complex, ParseComplexError]`, `parse_f64_or_throw`→`parse_f64_or_err`(Result), call-sites `!!`, neg-тест :593 расконсервирован). Баг 3 (`Result` над named-tuple) был причиной отката → **разрешён** `[M-181-result-over-named-tuple-codegen]` (`b022919a`, fix(172.1 codegen)). `nova test std/_experimental/math` → **complex = PASS** (end-to-end). Инфаллибл `from(f64)`/`from_imag`/`from_polar` — без изменений. NB: peer `statistics.nv` CC-FAIL пре-существующе/независимо (`assert (X).abs()` → abs на unit; не использует Complex). | малый, ✅ unblocked |
| `std/encoding/json.nv` ✅ **DONE (исходник, 2026-06-25)** | `Json.parse`/`Parser.*`/`Lexer.@read_*` → `Result[…, ParseJsonError]` (~15 fn; throw→Err/return Err; `?`-threading; `Lexer.@advance`/`@peek` = Option, без `?`); `JsonValue.from(str)`→`try_from`; ~35 `Json.parse` в тестах → `!!`, 5 neg `with Fail`-блоков → `Err(..)`-match. `nova check` ✅. **🟡 ОБА codegen-блокера сняты 2026-06-26** (баг 4 анон-record `c724de7a`; erasure self-ref `[]Self` `[M-172.1-self-ref-slice-variant-erasure]` `98fa5c56`) → **json теперь КОМПИЛИРУЕТСЯ** (`nova test` доходит до runtime). **🟢 object-тест ЗЕЛЁНЫЙ 2026-06-26** (`parse: object с полями` — корень был **sum-eq**: `Option[JsonValue]==` сравнивал указатели; чинит `[M-172.1-option-eq-heap-aggregate-structural]` `f53e32a9`; мутация `mut fields`/`.get` оказались звучны). **🟢 record-eq добит 2026-06-26** (`[M-172.1-option-eq-record-structural]` `917599e8`: `Option[<record>]==` / record-поле-в-sum / прямой `Rec==Rec` теперь структурно — затрагивает json `ParseJsonError`/record-варианты; завершает sum-фикс, единый диспетчер per-type-eq §0). **⚠ НЕ полностью зелёный:** 2 остаточных пре-существующих фейла — `into: array round-trip` (**container**-eq: `Array([..])==` нужна element-wise Vec-eq → `[M-172.1-option-container-eq-structural]`, ОТДЕЛЬНО от record-eq) + `parse: ошибка — trailing content` (**parser-логика**, детект trailing → `[M-181-json-trailing-content]`). Оба — отдельные follow-up'ы, НЕ регрессия (оригинал json не компилировался). | **большой** |

> **Общий caveat:** round-trip `s.into()`/`From[str]` для complex/json (получить тип из строки) опирается на **D77 4-way auto-derive** — это компилятор, **откладывается**. Сейчас вызывающие используют явный `Complex.try_from(s)` / `Json.parse(s)`. Инфаллибл `from` не трогаем. Все call-sites этих трёх — **внутри их собственных тестов** (cross-module потребителей нет).

> **🔬 Найдено при миграции base64 (2026-06-25) — 2 ПРЕ-СУЩЕСТВУЮЩИХ codegen-бага** (есть и в HEAD; `nova test std/encoding/base64.nv` никогда не проходил — `decode_*` режутся DCE, если decode не вызван, и codegen на файле штатно не гоняли). Подтверждено: подмена на HEAD-версию → тот же CC-FAIL; HEAD + только фикс бага 1 → всплывает баг 2.
> 1. **`decode_char` int/u8 mixing** — `Some(62)`/`Some(63)` (литералы → `Option[int]`) в одном if-выражении с `Some(.. as u8)` (`Option[u8]`) → codegen микширует `NovaOpt_nova_int`/`NovaOpt_nova_byte`. **✅ ИСПРАВЛЕНО в исходнике** (`Some(62 as u8)`/`Some(63 as u8)`, по стилю соседних веток).
> 2. **if-chain tail unit-cast в `decode_with`** — `out.push(...)` как последнее выражение ветки `tail==2` codegen типизирует как `out` (массив), соседнюю ветку — как `unit` → каст `unit → NovaArray_nova_byte*` = CC-FAIL. **Codegen-баг** (checker пропускает чисто; CC-FAIL = баг фронтенда по compiler-conventions §6). **Корень (owner-insight 2026-06-25):** `push` = `mut @`-метод; приёмник передаётся **по ссылке** (аналог `T&`, ABI-only — reference НЕ тип в Nova, значение не типизируется как «ссылка на X»). Материализатор значения if-ветки захватывает C-приёмник-указатель (`NovaArray*`) вместо `unit` (настоящего return-типа `push`) → клэш с unit-веткой. **Фикс:** материализатор берёт return-тип метода, не ссылку приёмника. Компилятор → **ОТЛОЖЕН**, маркер **`[M-181-ifexpr-value-materialize-codegen]`** (материализация значения if-выражения; **overlaps Plan 172.1** U.4.4 if-expr). До фикса полный `nova test` base64 невозможен; **исходник D325-корректен** (`nova check` ✅). *(base64 закоммичен: пре-существующий баг, не регрессия — оригинал тоже падал `nova test`.)*
> 3. **Result над named-tuple — codegen type-ordering** (complex.nv, **РЕГРЕССИЯ**) — ✅ **RESOLVED 2026-06-26** (`b022919a`, ветка plan-172): `Result[Complex, ParseComplexError]` (Complex = named-tuple `type Complex(re, im)`) → структура `NovaRes_NovaTuple_Complex_…` использовала `NovaTuple_Complex` ДО его typedef → `unknown type name 'NovaTuple_Complex'`. **Фикс** (`[M-181-result-over-named-tuple-codegen]`, зеркало NovaOpt VR-routing [M-153.2]): wrapper-body whose by-value payload — late-emitted named-tuple/value-record → в late-секцию `__NOVARES_VR_TYPEDEFS__` (после struct-bodies); forward-typedef остаётся рано. NB: «forward-декларация» исходной формулировки была неточна (by-value член требует ПОЛНЫЙ тип). Миграция complex.nv **re-applied** (`a2d01a67`), `nova test` complex = PASS.
> 4. **Анонимный record-литерал как аргумент `Ok(...)`** (json.nv) — ✅ **RESOLVED 2026-06-26** (`c724de7a`, ветка plan-172): `Ok({ tok, line, col })` / `Err({ why })` → `codegen error: anonymous record literal without spread not supported`. В оригинале `{ … }` возвращался напрямую (codegen коэрсил target-тип `TokenWithPos`/`Parser` из return-типа по D55 через `expected_record_type`); обёрнутый в `Ok(...)`, контекст = тип Result, не payload → анон-литерал терял target-struct. **Фикс** (`[M-181-anon-record-in-ctor-arg-codegen]`, **ЛОКАЛЬНЫЙ codegen target-propagation, НЕ полный RecordLit-резолвер** Plan 172.1 U.4.5): contextual Ok/Err-арм `emit_call` уже несёт разрешённый payload-C-тип из канала (`novares_ok_err(&rt)`) → ставит `expected_record_type` вокруг emit аргумента (зеркало D55). Byte-identical для не-анон-record аргументов. json **разблокирован ПАСТ** анон-record (теперь упирается в пре-существующий downstream erasure-баг `as_array() -> Option[[]JsonValue]`, [M-91.13] — **НЕ регрессия**, оригинал json уже падал `nova test`). Source-workaround (type-annotated binding до `Ok`) больше не нужен. **Остаётся для green json:** фикс erasure-бага [M-91.13] (вне scope Ф.2a).
>
> **Урок для плана (важно):** «`.nv`-only» (не трогает compiler-source) ≠ «codegen-clean». Все **3** проверенных Ф.2a-файла упёрлись в codegen-баги. **Регрессия только у complex** (зелёный→красный, откачен); **base64 и json — пре-существующе-красные** (закоммичены: D325-корректны + `nova check`-чисты, `nova test`-статус не ухудшен). **Разблокировка Ф.2a требовала codegen-фиксов** — **ВСЕ 4 закрыты 2026-06-26** (Plan 172.1, ветка plan-172): `[M-181-ifexpr-value-materialize-codegen]` (`836befcb`), `[M-181-result-over-named-tuple-codegen]` (`b022919a`), `[M-181-anon-record-in-ctor-arg-codegen]` (`c724de7a`), `[M-172.1-self-ref-slice-variant-erasure]` (`98fa5c56`, бывш. erasure `[M-91.13]`). **Статус Ф.2a:** base64 ✅ green, complex ✅ green (re-applied `a2d01a67`), **json КОМПИЛИРУЕТСЯ** но имеет 2 отдельных пре-существующих RUN-FAIL в object-парсинге (см. json-строку выше) — отдельный вопрос, НЕ codegen-блокер D325.

### 🔴 Ф.2b — compiler-gated, **ОТЛОЖЕНО** (нужен `emit_c.rs`)

| Файл | Блокер |
|---|---|
| `std/runtime/string/parse.nv` (`try_parse_int`→`parse_int`) | `emit_c.rs:38081-38082` + `:34040-34041` хардкодят C-тип возврата метода `parse_int` = `NovaOpt_nova_int`. .nv-переименование без их правки → **silent mis-type** (Nova-body Result vs хардкод Option), не чистая ошибка. Удаления bare(:24) и `_opt`(:63) сами по себе .nv-only, но бессмысленны без rename. |
| `std/runtime/read_buffer.nv` (`try_read_X`→`read_X`) | `emit_c.rs:37724-37753` хардкодит `read_X`→unboxed C-типы, `try_read_*`→Result. Переименование без правки → mis-type на каждом call-site. |
| `std/prelude/protocols.nv` (ретракт bare auto-derive) | D77 4-way в `emit_c.rs` (`try_from_targets`/`from_targets`). Декларации `TryFrom`/`TryInto` **не трогаем**; текст-конвенцию — отдельно под governance. |
| builtins `int.try_parse`/`f64.try_parse`(→Result), `char.try_from` | `emit_c.rs:27160/27224`. Цель Plan 171 (+ баг truncation `i8.try_from("999")→-25`). |
| `std/_experimental/encoding/hex.nv` | механически .nv-only, но `_experimental` → отложить (§9 Q3). |

### ✅ Уже conformant (без изменений)
`std/net/*`, Plan 180, `std/encoding/utf16.nv` (`from_utf16 -> Result` — эталон целевой формы), `std/runtime/string/core.nv` (`try_from_codepoint` уже Result; `from_bytes_*` намеренно инфаллибл — не fallible, не трогать).

### Отложенный `_experimental` (§9 Q3)
`sql.nv`(Db), `jwt`/`snowflake`/`ulid`/`bcrypt`/`retry` — TODO. Прим.: `retry.execute`/`in_transaction` внешний `Fail[E]` = forwarded (R5, легально); чинить только intrinsic `Db`-ошибки.

---

## 7. Фазы

- **Ф.0 — Discovery (grep-скоуп).** Точный список call-sites bare-`parse_int`/`read_X`/`_opt` + всех `Fail[` в публичных std-сигнатурах (минус forwarded R5). Размер миграции.
- **Ф.1 — D325 + конвенции.** D325 в `04-effects.md` (после D85) + индекс/gap; правки E1-E9. Гейт: sign-off §2 (§9 Q1) + governance (§9 Q2).
- **Ф.2a — `.nv`-only миграция (можно сейчас).** base64.nv → complex.nv → json.nv (§6 Ф.2a). Per-file loop (`nova check FILE` → fix). Каждый файл — отдельное подтверждение владельца.
- **Ф.2b — compiler-gated (ОТЛОЖЕНО).** parse.nv + read_buffer.nv rename + `emit_c.rs` хардкоды (38081/34040, 37724) + builtins (int/f64.try_parse→Result, char.try_from) + D77 4-way ретракт. Делать, когда разрешены изменения компилятора.
- **Ф.3 — Guard (§8).** Lint: «нет `Fail[` в публичной std-сигнатуре, кроме forwarded-из-closure (R5)».
- **Ф.5 — Docs/log.** `project-creation.txt` + `discussion-log.md` (nova-private) + `simplifications.md`. Cross-ref из 171/173/180.

---

## 8. Тесты / guards

- **Conformance-guard:** скрипт по `std/**.nv` — публичная сигнатура с `Fail[E]` для собственной ошибки = FAIL; `Fail[E]`, происходящий из `fn() … Fail[E]`-параметра (R5 forwarding) = OK. Ожидаемо после Ф.2: 0 нарушений в stable; известные — в `_experimental` (TODO).
- **Net regression:** `grep -L 'Fail\[' std/net/*.nv` — все без `Fail[`.
- **Behavior:** `parse_int("x")` → `Err(...)`; `parse_int("42")!!` → `42` (throw-форма работает); `parse_int("42").ok()` → `Some(42)`.
- **Negative:** старые имена `try_parse_int`/`parse_int_opt`/bare-`read_X` → `E_UNKNOWN_METHOD` (удалены).

---

## 9. Открытые вопросы (решение владельца)

- **Q1 — нейминг-правило §2 (R2/R3).** Подтвердить: обычное имя = Result-форма (`parse_int -> Result`); `try_` оставить **только** для пары infallible/fallible (`from`/`try_from`)? (Рек.: да — Rust-консистентно, минимум префиксов.)
- **Q2 — governance.** Применять E1-E9 + D325 live сейчас, или staged до твоего ревью? (Ранее выбрано: **staged / сначала ревью**.)
- **Q3 — `_experimental`.** Отложить с TODO (рек., ранее выбрано) или мигрировать `sql.nv` Db сейчас?
- **Q4 — Plan 171.** 171 проектировал `from`/`try_from` + `try_parse`(Option). Под Вариантом 1: `try_parse`→`parse -> Result`, decimal через `try_from`(Result). **Выровнять 171 под §2** в рамках 181, или 181 задаёт правило, а 171 его применяет отдельным заходом? (Рек.: 181 задаёт D325, 171 реализует per-type — синхронизировать тексты.)

---

## 10. Координация

- **Plan 171** (primitive parse) — §9 Q4. 181 = правило (D325); 171 = per-type реализация под Result.
- **Plan 172.3** (type-set bounds) — ортогонально (схлопывает per-type обёртки в generic; нейминг общий из D325).
- **Plan 173** (error-machinery: defer-kernel/MultiError/structured-concurrency) — нейминга не касается; 181 лишь снимает stale net-pointer (E8). `Fail`-эффект, `!!`/`?` — общие, не трогаем.
- **Plan 180** (io/fs/os) — уже Result; 181 фиксирует, что он conformant; снять формулировки «по net-семейству как исключение».

## 11. Не в scope

- Реализация Fs/Io/Os (**Plan 180**); defer-kernel/MultiError (**Plan 173**); операторы `!!`/`?`/`??` (**D85**, стабильны).
- **Удаление эффекта `Fail` из языка** — НЕ делаем; он остаётся для пользовательского кода и внутренних хелперов.
- Стабилизация `_experimental` сверх fallible-контракта.
