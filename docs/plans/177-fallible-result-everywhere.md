<!-- SPDX-License-Identifier: CC-BY-4.0 -->
# Plan 177 — Единый fallible-контракт std: **Result-everywhere** (no bare-throws convention)

> **Top-level план.** Создан 2026-06-25. **Ред. 2 — 2026-07-03** (аудит: ground-truth/7-языков/blast-radius; статусы E-пунктов, R0-граница, spec_tests, агент-правила).
> **Статус:** ✅ **CLOSED 2026-07-04** (D325-конвенция полностью в спеке + guard + conformance 41/0; stable-std in-scope мигрирована; остаток — явные маркеры, см. §14) — **D325 ✅ committed** + **amend-пакет §4a ✅ внесён** (`04-effects.md`: R0/R4-критерий/nesting-канон/exempt-list/коллекторы, 2026-07-03); **Ф.1 ✅ DONE** (E1-E11 закрыты: E3/E6/E10/E11 добиты 2026-07-03 — read_config→Result, parse_int_opt→genuine-absence+cross-domain/wrap-Fail идиомы, D178 retract-баннер, D77 cross-ref); Ф.2a ✅ DONE; **Ф.2c ✅ DONE 2026-07-04 — ЦЕЛЕВОЙ ФОРМОЙ (`[M-172.1-U4-freefn-generic-return]`, `3ef1acb8`+`b8fb952d`): `sequence`/`partition` в `std/prelude/core.nv`, оба cross-module; codegen снят ЕДИНЫМ каналом — `f1_check_call` материализует конкретный generic-free-fn call-return через `unify_type`+subst → codegen читает `resolved_type_to_c` (D315); interim-зеркало `4e4e7c34` откачено net-zero; conformance 38/38 + zero-regression 23 папки); **Ф.2b ✅ DONE 2026-07-04 (`[M-177-Ф2b-parse-read-dehardcode]`): parse.nv/read_buffer.nv → единая Result-форма + де-хардкод их return-типов (§3/§10, резолв из .nv) + sweep; zero-regression vs `19b9c756`; де-хардкод доказан звучным ДО удаления (Channel-2 конкретный NovaRes). Отложено: D77-codegen (`[M-177-d77-codegen-4way-retract]`), builtins→174.1, spec-neg блок. на pre-existing `[M-checker-unknown-method-stackoverflow]` — **разблокирован fix'ом `d0208346`**)**; **Ф.3 ✅ DONE 2026-07-04**: conformance PASS 41/0 (pos `d325_result_everywhere` + 3 neg `d325_*`), guard+нейм-линт (`compiler-codegen/tests/d325_result_everywhere_guard.rs`) 3 passed / 0 нарушений stable std, A1-A4 покрыты, zero-regression by construction; отложено `d77`-spec-fixture + `[M-172.1-opt-result-over-userenum-typedef-order]`; **Ф.4 ✅ DONE 2026-07-04** — docs close-out + честный аудит полноты D325 (карта §14): stable-std public-fallible = **Result-everywhere** (мигрировано base64/json/complex/parse/read_buffer; conformant net/176/utf16/string-core; exempt-list §2 — unwrap-мост/on_exit/testing по-дизайну); **остаток честно маркирован** — (a) `std/concurrency` `race2`/`with_timeout` throw bare-`str` (inferred-Fail, вне guard-скана `Fail[`) = Plan 173-домен `[M-177-concurrency-throw-fallibility]`; (b) весь `std/_experimental` (17 файлов) = §9 Q3 defer до стабилизации `[M-177-experimental-fallible-migration]`; (c) codegen-хвост `[M-177-d77-codegen-4way-retract]` / `[M-172.1-opt-result-over-userenum-typedef-order]`. **ПЛАН 177 ЗАКРЫТ** (D325-конвенция достигнута полностью; несделанное — только явные followup-маркеры/делегации, §12.1).
> **Маркер:** `[M-177-result-everywhere-std]`. **Запуск:** «**выполни план 177**».
> **Очередность (граф 173-181 — [README планов §Очередность](README.md), 2026-07-03):** D325 ✅ уже в спеке
> (Ф.1-ядро выполнено). Migration-sweep (read_buffer bare-twins, emit_c builtins) — Волна 1 трек G,
> независим. **Ф.2b (rename SHIPPED parse-триады) — Волна 3**: тройная координация 172.1×174.1×177
> (174.1 создаёт новые поверхности сразу под D325-именами — 174 §3.1).
> **Решение (2026-06-25):** **Вариант 1** — **вся публичная std возвращает `Result[T, E]`** на любой падающей операции.
> Дуальный `bare`(throw)/`try_`(Result)/`_opt`(Option)-нейминг **РЕТРАКТИРУЕТСЯ** из std. Эффект `Fail[E]` **остаётся в языке**
> (для пользовательского кода и внутренних хелперов), но публичный std-API его для своих ошибок не несёт. throw на call-site = `!!`, проброс = `?`, ветвление = `match`, `Result→Option` = `.ok()`.
> **D-блок:** **D325 ✅ committed** (`04-effects.md`) — единое правило; **amends/retracts D77** (4-way auto-derive bare-формы; ⚠ spec-часть D77-ретракта уже внесена независимо 2026-07-01 — см. §3/E11) и **D178** (`parse_int` bare + `parse_int_opt`; ⚠ retract-баннер в 08-runtime ещё НЕ проставлен — E10). Ред.2 добавляет **amend-пакет** к D325 (§4a: R0-граница D13, R4-критерий, exempt-list, nesting-канон).
> **Эталон (живой код):** [std/net](../../std/net/effect.nv) — уже Result-everywhere, 0 `Fail[`. Под Вариантом 1 это **просто норма**, а не «исключение».
> **Координация:** **Plan 174.1** (primitive parse — выровнять под Result, §10), Plan 172.3 (type-set bounds — ортогонально), Plan 176 (io/fs/os — уже Result; + env-edge R4, §10), Plan 173 (error-MACHINERY — нейминга не касается; Ф.1 делает `?` return-only — совместимо).
> **Решение принято осознанно** на этапе до-прода: «спроектировать правильно сейчас, переделать сделанное, если нужно» — причина объективная (см. §1), не sunk-cost.

---

## 0. TL;DR

1. **Одно правило на язык для std:** любая падающая публичная операция → **`Result[T, E]`**. Без bare-throws-близнецов, без `try_`-дублей, без `_opt`.
2. **Эффект `Fail[E]` не удаляется** — он остаётся механизмом языка. Просто std им свои ошибки наружу не отдаёт. Хочешь throw-стиль в **своём** коде — пиши `Fail[E] -> T`, язык позволяет.
3. **Эргономика throw сохранена операторами:** `expr!!` (throw из Result), `expr?` (проброс), `expr.ok()` (→Option), `match`.
4. **Нейминг (§2):** обычное имя = Result-форма (`parse_int -> Result`). Префикс `try_` — **только** чтобы отличить fallible-вариант от одноимённого **infallible** (`from`/`try_from`). `Option` — только для genuine absence (`find`/`get`/`env`), не для fallibility.
5. **Граница с panic (R0, D13):** contract-violation / programming error (OOB, overflow, div0, mid-codepoint slice) — **panic**, НЕ Result и НЕ Fail. «Падающая» в п.1 = expected/environmental failure.
6. **Ретракции:** D77 4-way (bare-форма конверсий), D178 (`parse_int` bare + `parse_int_opt`), nv-coding-style §4 дуал-булет, само понятие «двух категорий».
7. **Миграция:** `parse.nv` (`try_parse_int`→`parse_int`, удалить bare+`_opt`), `read_buffer.nv` (`try_read_X`→`read_X`, удалить **22** bare-twin), `emit_c.rs` builtins, call-sites (sweep-списки §6). net/Plan 176 — без изменений. `_experimental` — отложить с TODO (§9 Q3).

---

## 1. Контекст — почему Вариант 1 (объективно, не sunk-cost)

Развилка прошла три шага: **A** (bare=throws everywhere) → **B1** (две категории: I/O=Result, scalar=дуал) → **Вариант 1** (всё Result). A отвергнут (close-footgun на must-consume); B1 отвергнут как «слишком сложно для рядового разработчика + вечная граница». Объективные причины за Вариант 1 (на годы, не «жалко переделывать»):

1. **`Result` безопасен в 100% операций; bare-throws — нет.** Мы сами запретили bare-throws для файлов (`close` глотает ошибку → потеря данных, [176:29-32](176-io-fs-os.md)). Примитив, которому нужна **граница**, чтобы быть безопасным, слабее универсального.
2. **Нет границы — нет вечного налога.** Две категории = бесконечная серия «а это куда?» (snowflake был первым). Одно правило → вопроса не существует; компилятор не должен охранять рубеж, которого нет.
3. **Ошибка-как-значение фундаментальнее, чем как-throw.** `Result` кладётся в `Vec`, мапится, собирается (`[]Result` при построчном разборе — коллекторы Ф.2c), возвращается из замыкания, шлётся в канал. Брошенный `Fail` — это control-flow, как данные не используется. Где нужна «ошибка как данные» — всё равно нужен `Result`. Значит `Result` — то, без чего не обойтись; bare-throws — необязательная надстройка.
4. **Меньше имён на операцию.** Сейчас один разбор = до трёх имён (`parse_int`/`try_parse_int`/`parse_int_opt`). Одно имя + операторы — меньше поверхности, доков, путаницы «какой звать».
5. **`!!` уже даёт throw, когда нужен.** Реальная потеря — 2 символа на проброс в скриптах, и только там.

> **Сознательный trade-off #1 (краткость):** теряем operator-free краткость в glue-скриптах (`read_file(p)` vs `read_file(p)!!`). Принято: единообразие и композируемость std важнее на дистанции десятилетий. Эффект `Fail` остаётся в языке → скрипт-стиль доступен в пользовательском коде, просто не навязан std.
>
> **Сознательный trade-off #2 (map_err-налог, честно):** авто-`From`-конверсия ошибок при `?` **отклонена** (D85 amend 174.2, `04-effects.md:4444`) → cross-domain композиция (например, `IoError` + `ParseIntError` в одной fn) требует `.map_err(...)` на каждом сайте либо явный domain-sum-error. Это дороже «2 символов» — паттерн канонизируется в E6 (§5). Обратное направление (обернуть Fail-код в Result) — идиома `with Fail[E] = |e| interrupt Err(e) { … }` (`04-effects.md:1114`), аналог Kotlin `runCatching` / Swift `Result(catching:)` — тоже канонизируется в E6.

### 1a. Планка 7 языков (Rust/Go/TS/Kotlin/Java/Zig/Swift)

| Язык | Модель ошибок | Урок для Nova |
|---|---|---|
| **Zig** | `!T` (error union) обязателен; не-Result пути **нет вовсе** — `try`≡`?`, `catch`≡`!!`/`??`; panic — отдельный канал | **Сильнейший прецедент Варианта 1**: современный системный язык живёт вообще без throw-канала |
| **Rust** | std = `Result` + panic-for-bugs; `?`; `#[must_use]` | Прямая модель Nova; граница panic/Result = наш R0/D13 |
| **Go** | `(T, error)` **без** must-use → в проде обязателен `errcheck`-линтер | must-consume `Result` Nova строже by-construction — аргумент ЗА |
| **Swift** | throws-первичен; `Result` — адаптер (`Result(catching:)`); typed throws только с Swift 6 | Вечный bridging-налог двух миров — цена НЕ-единообразия |
| **Kotlin** | `kotlin.Result` изначально **запрещён** как return-тип публичного API (до сих пор discouraged) | Цена полумеры: Result есть, но не канон → экосистема остаётся на исключениях |
| **Java** | checked exceptions = признанный провал (wrapper-hell; несовместимость с лямбдами/Streams; никем не повторён) | throws-в-сигнатуре без value-семантики не композируется |
| **TS** | untyped `throw`; typed-throws proposal заглох; экосистема компенсирует neverthrow/fp-ts | Untyped-канал не лечится постфактум |

Все три альтернативы Варианту 1 — throws-primary (Swift), checked-throws (Java), untyped (TS/JS) — имеют **документированные провалы**; таблица преэмптит relitigating «а почему не throws-первично».

---

## 2. 🎯 ЯДРО — единое правило нейминга

**(R0) Граница panic vs Result (D13).** «Падающая операция» в R1 = **expected/environmental failure** (пользовательский ввод, I/O, парсинг, ресурсы среды). **Contract-violation / programming error → panic** per D13 (`spec/decisions/08-runtime.md:124-140`), НЕ Result и НЕ Fail. Пары-примеры: `v[i]` OOB → panic / `v.get(i)` → Option; integer overflow, div/0 → panic; `s[a..b]` mid-codepoint → panic / `parse_int` → Result. Прецедент: Rust и Zig оба держат panic/unreachable **вне** error-канала. Без R0 правило R1 читалось бы как «Result вместо panic» — это не так.

**(R1) Любая падающая публичная операция std → `Result[T, <Domain>Error]`.** Один структурный `XError` на домен.

**(R2) Имя — обычное, без маркера-префикса.** `str.parse_int(s) -> Result[int, ParseIntError]`, `rb.read_u32() -> Result[u32, ReadBufferError]`, `File.open(p) -> Result[File, IoError]`. (Как Rust `str::parse -> Result`.)

**(R3) Префикс `try_` — ТОЛЬКО для дизамбигуации fallible-варианта одноимённого infallible.** Существует `from` (infallible, D73) → fallible-вариант = `try_from` (Result, D77). `into` → `try_into`. Здесь `try_` маркирует **«может упасть»** относительно чистого `from`, а не «Result-близнец bare-формы». В одиночных fallible-операциях (нет infallible-сиблинга) префикса НЕТ.

**(R4) `Option` — только для genuine absence, не для fallibility.** Критерий-тест (одна фраза): **>1 причины отказа ИЛИ вызывающему нужна причина → `Result`; единственный нормальный исход «нет» → `Option`.** `find`/`get`/`parent`/`Metadata.modified` → `Option`. `Result → Option` через `.ok()`. **Никаких `_opt`-имён.** Edge-и:
- `env` — числится в Option-списке, но если у Nova есть non-unicode путь (Windows), это Result (Rust `env::var = Result[VarError]{NotPresent, NotUnicode}`) либо документированная lossy-гарантия — **решает Plan 176** (координация §10);
- **fallible-итерация — канон вложенности:** `@next() -> Option[Result[T, E]]` (Rust-модель: exhaustion снаружи как Option, ошибка элемента внутри как Result). Формулировка «DirIter.next -> Result[Item, E]» из первой редакции **уточняется** amend-пакетом §4a.

**(R5) Эффект `Fail[E]` в публичном std-API — запрещён для СОБСТВЕННЫХ ошибок, но разрешён для прозрачного проброса пользовательского.** Higher-order-функция, прокидывающая `Fail[E]` из closure-параметра, эффект-полиморфна и легальна:
```nova
// ЛЕГАЛЬНО: Fail[E] forwarded из тела пользователя (не своя ошибка std):
fn retry[T, E](body fn() Fail[E] -> T, policy RetryPolicy) Time Random Fail[E] -> T
// НЕЛЕГАЛЬНО под R1: своя ошибка std через throw:
fn Db.query(q Sql) Db Fail[DbError] -> []DbRow      // → Result[[]DbRow, DbError]
```
Дискриминатор для guard'а (§8): несёт ли сигнатура `Fail[E]`, **происходящий из `fn() … Fail[E]`-параметра** (forwarded — ок), или это собственная ошибка функции (→ обязан Result).

**Explicit exempt-list (для guard'а §8 и текста D325, — иначе false-positive):**
- `std/prelude/core.nv` — `extern Option@unwrap` / `Result@unwrap` c `Fail[...]` (:255, :354) — это **сам D85-мост `!!`**, by-design;
- `std/prelude/protocols.nv` — protocol-member `on_exit(...) Fail[E]` (:460) — user-`E`, R5-forwarding;
- `std/testing/property.nv` — 4 export-сигнатуры с `Fail` (Q5 ✅ **exempt**, sign-off 2026-07-03: assert/test-DSL-семантика — «упади сейчас» и есть смысл assert'а).

**Эталон:** [std/net](../../std/net/tcp.nv) — Result-everywhere, 0 `Fail[`. **Под-паттерны (conformant):** per-element итерация → `Option[Result[T,E]]`; absence → `Option`; инфаллибл-аксессоры → чистое значение.

> Граница «I/O vs scalar» из B1 **упразднена** — её больше нет ни в правиле, ни в коде, ни в голове разработчика.

---

## 3. Ретракции (что откатываем)

| Решение | Было | Под Вариантом 1 | Статус |
|---|---|---|---|
| **D77** (TryFrom/TryInto) | 4-way auto-derive: из `try_from`(Result) компилятор генерит bare `from`(throws) | **Amend:** убрать авто-генерацию bare-throws fallible-формы. Остаются `from`(infallible) + `try_from`(Result). «4-way» → «2-way». | ⚠ **Spec-часть УЖЕ сделана независимо 2026-07-01** (`08-runtime.md:1662-1667`: «unified 4-way auto-derive отозван», D73/D77 = две отдельные иерархии; без cross-ref на D325 — E11). Остался **только emit_c** (`from_targets`/`try_from_targets`-синтез) — Ф.2b. |
| **D178** (str.parse_int) | `parse_int`(bare throws) + `try_parse_int`(Result) + `parse_int_opt`(Option) | **Retract bare + `_opt`:** одна форма `parse_int -> Result`; Option через `.ok()`. | ❌ **Retract-баннер НЕ проставлен**: D178 V2 (`08-runtime.md:4334`) и V3 (:4433-4434) стоят как есть, 0 упоминаний D325 в файле → **E10**. |
| **nv-coding-style §4** дуал-булет (83-91) | «дуал bare/try_ — общая конвенция» | **Retract:** заменить на R1-R5 (единое Result-правило). | ✅ DONE (E1). |
| **nv-coding-style §4** net-carve-out (92-94) | «net — открытый вопрос, Plan 173 унифицирует» | **Delete:** под Вариантом 1 net — просто норма; carve-out не нужен. | ✅ DONE (E2). |
| **«две категории» (бывш. B1)** | Cat 1 / Cat 2 + граница | **Удалить концепцию целиком.** | ✅ DONE. |
| **D25** (`Fail[E]` throw) | механизм throw/Fail | **Без изменений** — остаётся в языке; меняется только std-конвенция (не использовать наружу). | — |
| **D85** (`?`/`!!`/`??`) | операторы | **Без изменений** — несущая эргономика Варианта 1. | — |

---

## 4. D325 — ✅ committed (`04-effects.md`, после D85; Status ACTIVE, 2026-06-25)

Текст первой редакции (R1-R5, эталон std/net, под-паттерны) — внесён в спеку; draft ниже сохранён как историческая справка соответствия.

```
## D325 — Единый fallible-контракт: публичный std возвращает Result

Статус: проектное решение (Plan 177, 2026-06-25). Amends D77 (убрать bare auto-derive),
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

### 4a. Amend-пакет Ред. 2 к D325 (внести в спеку, Ф.1-остаток)

1. **R0-граница (D13):** cross-link на D13 + фраза «expected/environmental → Result; contract-violation/programming error → panic, НЕ Result и НЕ Fail» + пары-примеры (§2 R0).
2. **R4-критерий:** «>1 причины отказа ИЛИ нужна причина → Result; единственный нормальный исход "нет" → Option» + edge `env` (делегирован 176).
3. **Nesting-канон fallible-итерации:** `@next() -> Option[Result[T,E]]`; поправить под-паттерн «DirIter.next -> Result[Item,E]» в тексте D325.
4. **Exempt-list (§2):** `Option@unwrap`/`Result@unwrap` (D85-мост), `on_exit` (R5), `testing/property` (по Q5-решению).
5. **Коллекторы:** одно предложение «работа с `[]Result` — `sequence`/`partition` (prelude, Plan 177 Ф.2c)».

> **Гигиена D-нумерации:** D316–D324 зарезервированы планами 175/175.1/176; reserved-gap отмечен в `spec/decisions/README.md:140`. Amend-пакет НЕ занимает новый номер — это врезка в существующий D325.

---

## 5. Правки конвенций (E1-E11; статусы — аудит 2026-07-03)

> 🔒 Конвенции нормативны. Governance-модель (`conventions-governance.md:28-32`): отдельный changelog НЕ ведём — дата согласования **инлайн** `· согласовано YYYY-MM-DD` у пункта.

- **(E1)** ✅ DONE — `nv-coding-style.md` §4 :83-90 = R1-R5 (единое Result-правило); инлайн-дата `· согласовано 2026-06-25` добавлена 2026-07-03.
- **(E2)** ✅ DONE — net-carve-out удалён.
- **(E3)** ✅ DONE (2026-07-03) — `nv-coding-style.md` §20.4 :640 `read_config` переписан в Result-форму (`Fs -> Result[Config, IoError]`; `Fs.open(path)?` + `defer file.close()` + `read_all()?` + `Ok(Config.parse(raw))`), defer-иллюстрация сохранена. Целевой снимок:
  ```nova
  fn read_config(path str) Fs -> Result[Config, IoError] {
      consume file = Fs.open(path)? {        // ? разворачивает Result; File must-consume → consume-scope
          ro raw = file.read_all()?
          Config.parse(raw)                  // close-Result сворачивается on_exit'ом (ENOSPC виден)
      }
  }
  ```
- **(E4)** ✅ DONE — `module-conventions.md` §3 :92 («Все fallible → Result (R1, D325)»).
- **(E5)** ✅ DONE — `module-conventions.md` §5 :151-152 (`try_`-дуал убран).
- **(E6)** ✅ DONE (2026-07-03) — `idioms/error-handling.md` :28 `s.parse_int_opt()` заменён на genuine-absence `m.get(key) -> Option[V]` + пояснение (fallible-с-причиной → Result+`.ok()`, `_opt`-близнеца нет, R4). Дописаны идиомы: (a) **cross-domain композиция** — `.map_err` per-site + domain-sum-error `enum ConfigError Io | Parse`; (b) **wrap Fail→Result** `with Fail[E] = |e| interrupt Err(e) { … }` (runCatching-style). `strings.md` — ✅ ранее.
- **(E7)** ✅ DONE — `std/prelude/protocols.nv` :126-143 (текст-конвенция D77 2-way).
- **(E8)** ✅ MOOT — claim «Plan 173 унифицирует net-нейминг» в 173 уже отсутствует (grep = 0); 173 Ф.1 делает `?` строго return-only — примеры/конвенции 177 уже совместимы.
- **(E9)** ✅ RESOLVED (переформулирован Ред.2) — «дата-запись в conventions-governance.md» невыполнима как написано (там changelog не ведут); правильная форма = **инлайн-даты** у изменённых пунктов. Дата у D325-булета nv-coding-style:83 проставлена (E1).
- **(E10)** ✅ DONE (2026-07-03) — retract-баннер D178: `08-runtime.md` D178 V1-хедер (⚠ ЧАСТИЧНО RETRACTED — parse_int-триада отозвана D325, остальной str-cleanup в силе) + врезка на V3 :4433-4434 (amend V4: `parse_int -> Result`, `.ok()`, `!!`; удаление bare+`_opt` = Ф.2b). Спека больше не противоречит D325.
- **(E11)** ✅ DONE (2026-07-03) — D77 ревизия-баннер (`08-runtime.md:1662`) теперь ссылается на D325 (amends D77 4-way→2-way; синтез в emit_c → Ф.2b).

---

## 6. Миграция std — разбита на **.nv-only (сейчас)** и **compiler-gated (отложено)**

> 🔑 Решение владельца: правки **только в `.nv`** — можно сейчас (с подтверждением каждой); всё, что трогает **компилятор** (`emit_c.rs`) — откладываем. Discovery-workflow (`discover-v1-nv-only-migration`, 2026-06-25) классифицировал каждый пункт.

### 🟢 Ф.2a — `.nv`-only, можно сейчас (без компилятора)

| Файл | Действие | Размер |
|---|---|---|
| `std/encoding/base64.nv` ✅ **DONE (исходник, 2026-06-25)** | `Base64.decode`/`decode_url` → `Result[[]u8, Base64Error]`; `decode_with` → Result; `decode_or_throw`→`decode_at` (Result) + `?`; `throw`→`Err`/`return Err`; тесты :339-359 (`!!` для success, `Err(...)`-match для neg). `nova check` ✅. **⚠ полный `nova test` блокирован 2 пре-существующими codegen-багами (см. ниже)** — не от миграции. | **малый, самый чистый** |
| `std/_experimental/math/complex.nv` ✅ **DONE 2026-06-26** (`a2d01a67`, ветка plan-172) | Миграция re-applied после codegen-фикса (`Complex.from(s str)`→`Complex.try_from(s) -> Result[Complex, ParseComplexError]`, `parse_f64_or_throw`→`parse_f64_or_err`(Result), call-sites `!!`, neg-тест :593 расконсервирован). Баг 3 (`Result` над named-tuple) был причиной отката → **разрешён** `[M-177-result-over-named-tuple-codegen]` (`b022919a`, fix(172.1 codegen)). `nova test std/_experimental/math` → **complex = PASS** (end-to-end). Инфаллибл `from(f64)`/`from_imag`/`from_polar` — без изменений. NB: peer `statistics.nv` CC-FAIL пре-существующе/независимо (`assert (X).abs()` → abs на unit; не использует Complex). | малый, ✅ unblocked |
| `std/encoding/json.nv` ✅ **DONE (исходник, 2026-06-25)** | `Json.parse`/`Parser.*`/`Lexer.@read_*` → `Result[…, ParseJsonError]` (~15 fn; throw→Err/return Err; `?`-threading; `Lexer.@advance`/`@peek` = Option, без `?`); `JsonValue.from(str)`→`try_from`; ~35 `Json.parse` в тестах → `!!`, 5 neg `with Fail`-блоков → `Err(..)`-match. `nova check` ✅. **🟡 ОБА codegen-блокера сняты 2026-06-26** (баг 4 анон-record `c724de7a`; erasure self-ref `[]Self` `[M-172.1-self-ref-slice-variant-erasure]` `98fa5c56`) → **json теперь КОМПИЛИРУЕТСЯ** (`nova test` доходит до runtime). **🟢 object-тест ЗЕЛЁНЫЙ 2026-06-26** (`parse: object с полями` — корень был **sum-eq**: `Option[JsonValue]==` сравнивал указатели; чинит `[M-172.1-option-eq-heap-aggregate-structural]` `f53e32a9`; мутация `mut fields`/`.get` оказались звучны). **🟢 record-eq добит 2026-06-26** (`[M-172.1-option-eq-record-structural]` `917599e8`: `Option[<record>]==` / record-поле-в-sum / прямой `Rec==Rec` теперь структурно — затрагивает json `ParseJsonError`/record-варианты; завершает sum-фикс, единый диспетчер per-type-eq §0). **🟢 array round-trip добит 2026-06-26** (`[M-172.1-option-container-eq-structural]` `f56cd7b7`: `Array([..])==Array([..])` теперь element-wise через mono `Vec____Nova_JsonValue_p_method_equal`). **🟢 nested object round-trip добит 2026-06-26** (`[M-172.1-option-hashmap-eq-structural]` `bd56022e`: написан `HashMap[K,V] @equal` Nova-body + `Object→HashMap____nova_str__Nova_JsonValue_p_method_equal`; json.c JsonValue eq доказывает — `Array→mono Vec eq`, `Object→mono HashMap eq`, оба контейнера). **🟢 trailing-content добит 2026-06-26** (`[M-177-json-trailing-content]` `99077fbc`: `next_token` нераспознанный char → токен-сентинел `BadTok(char)` вместо hard `Err` → value-завершающий префетч успешен → top-level trailing-проверка даёт TrailingContent, mid-structure → UnexpectedChar через catch-all'ы; таксономия сохранена). **✅ json ПОЛНОСТЬЮ ЗЕЛЁНЫЙ 2026-06-26: std/encoding/json PASS:1 FAIL:0** (все ~35 parse + round-trip + 5 neg + trailing/nested-object/array). plan91_13 потребители PASS:1. **Plan 177 Ф.2a json завершён end-to-end** — все блокеры сняты (4 codegen + sum/record/Vec/HashMap eq + parser trailing). | **большой** |

> **Общий caveat:** round-trip `s.into()`/`From[str]` для complex/json (получить тип из строки) опирается на **D77 4-way auto-derive** — это компилятор, **откладывается**. Сейчас вызывающие используют явный `Complex.try_from(s)` / `Json.parse(s)`. Инфаллибл `from` не трогаем. Все call-sites этих трёх — **внутри их собственных тестов** (cross-module потребителей нет).

> **🔬 Найдено при миграции base64 (2026-06-25) — 2 ПРЕ-СУЩЕСТВУЮЩИХ codegen-бага** (есть и в HEAD; `nova test std/encoding/base64.nv` никогда не проходил — `decode_*` режутся DCE, если decode не вызван, и codegen на файле штатно не гоняли). Подтверждено: подмена на HEAD-версию → тот же CC-FAIL; HEAD + только фикс бага 1 → всплывает баг 2.
> 1. **`decode_char` int/u8 mixing** — `Some(62)`/`Some(63)` (литералы → `Option[int]`) в одном if-выражении с `Some(.. as u8)` (`Option[u8]`) → codegen микширует `NovaOpt_nova_int`/`NovaOpt_nova_byte`. **✅ ИСПРАВЛЕНО в исходнике** (`Some(62 as u8)`/`Some(63 as u8)`, по стилю соседних веток).
> 2. **if-chain tail unit-cast в `decode_with`** — `out.push(...)` как последнее выражение ветки `tail==2` codegen типизирует как `out` (массив), соседнюю ветку — как `unit` → каст `unit → NovaArray_nova_byte*` = CC-FAIL. **Codegen-баг** (checker пропускает чисто; CC-FAIL = баг фронтенда по compiler-conventions §6). **Корень (owner-insight 2026-06-25):** `push` = `mut @`-метод; приёмник передаётся **по ссылке** (аналог `T&`, ABI-only — reference НЕ тип в Nova, значение не типизируется как «ссылка на X»). Материализатор значения if-ветки захватывает C-приёмник-указатель (`NovaArray*`) вместо `unit` (настоящего return-типа `push`) → клэш с unit-веткой. **Фикс:** материализатор берёт return-тип метода, не ссылку приёмника. Компилятор → **ОТЛОЖЕН**, маркер **`[M-177-ifexpr-value-materialize-codegen]`** (материализация значения if-выражения; **overlaps Plan 172.1** U.4.4 if-expr). До фикса полный `nova test` base64 невозможен; **исходник D325-корректен** (`nova check` ✅). *(base64 закоммичен: пре-существующий баг, не регрессия — оригинал тоже падал `nova test`.)*
> 3. **Result над named-tuple — codegen type-ordering** (complex.nv, **РЕГРЕССИЯ**) — ✅ **RESOLVED 2026-06-26** (`b022919a`, ветка plan-172): `Result[Complex, ParseComplexError]` (Complex = named-tuple `type Complex(re, im)`) → структура `NovaRes_NovaTuple_Complex_…` использовала `NovaTuple_Complex` ДО его typedef → `unknown type name 'NovaTuple_Complex'`. **Фикс** (`[M-177-result-over-named-tuple-codegen]`, зеркало NovaOpt VR-routing [M-153.2]): wrapper-body whose by-value payload — late-emitted named-tuple/value-record → в late-секцию `__NOVARES_VR_TYPEDEFS__` (после struct-bodies); forward-typedef остаётся рано. NB: «forward-декларация» исходной формулировки была неточна (by-value член требует ПОЛНЫЙ тип). Миграция complex.nv **re-applied** (`a2d01a67`), `nova test` complex = PASS.
> 4. **Анонимный record-литерал как аргумент `Ok(...)`** (json.nv) — ✅ **RESOLVED 2026-06-26** (`c724de7a`, ветка plan-172): `Ok({ tok, line, col })` / `Err({ why })` → `codegen error: anonymous record literal without spread not supported`. В оригинале `{ … }` возвращался напрямую (codegen коэрсил target-тип `TokenWithPos`/`Parser` из return-типа по D55 через `expected_record_type`); обёрнутый в `Ok(...)`, контекст = тип Result, не payload → анон-литерал терял target-struct. **Фикс** (`[M-177-anon-record-in-ctor-arg-codegen]`, **ЛОКАЛЬНЫЙ codegen target-propagation, НЕ полный RecordLit-резолвер** Plan 172.1 U.4.5): contextual Ok/Err-арм `emit_call` уже несёт разрешённый payload-C-тип из канала (`novares_ok_err(&rt)`) → ставит `expected_record_type` вокруг emit аргумента (зеркало D55). Byte-identical для не-анон-record аргументов. json **разблокирован ПАСТ** анон-record (теперь упирается в пре-существующий downstream erasure-баг `as_array() -> Option[[]JsonValue]`, [M-91.13] — **НЕ регрессия**, оригинал json уже падал `nova test`). Source-workaround (type-annotated binding до `Ok`) больше не нужен. **Остаётся для green json:** фикс erasure-бага [M-91.13] (вне scope Ф.2a).
>
> **Урок для плана (важно):** «`.nv`-only» (не трогает compiler-source) ≠ «codegen-clean». Все **3** проверенных Ф.2a-файла упёрлись в codegen-баги. **Регрессия только у complex** (зелёный→красный, откачен); **base64 и json — пре-существующе-красные** (закоммичены: D325-корректны + `nova check`-чисты, `nova test`-статус не ухудшен). **Разблокировка Ф.2a требовала codegen-фиксов** — **ВСЕ 4 закрыты 2026-06-26** (Plan 172.1, ветка plan-172): `[M-177-ifexpr-value-materialize-codegen]` (`836befcb`), `[M-177-result-over-named-tuple-codegen]` (`b022919a`), `[M-177-anon-record-in-ctor-arg-codegen]` (`c724de7a`), `[M-172.1-self-ref-slice-variant-erasure]` (`98fa5c56`, бывш. erasure `[M-91.13]`). **Статус Ф.2a: ✅ все 3 файла green end-to-end** (json добит 2026-06-26, см. json-строку).

### 🟢 Ф.2b — parse/read-триады SHIPPED + де-хардкод ✅ DONE (2026-07-04); D77-codegen + builtins отложены

> **✅ DONE 2026-07-04 (`[M-177-Ф2b-parse-read-dehardcode]`):** parse.nv + read_buffer.nv
> переименованы в **единую Result-форму** (D325) с **де-хардкодом их return-типов** (§3/§10 —
> резолв из `.nv`, НЕ «поправить хардкод»). Ключевая проверка звучности (§10-порядок): доказано
> ЭМПИРИЧЕСКИ до удаления, что чекер материализует конкретный Result в канал, а codegen читает
> его ЕДИНЫМ `resolved_type_to_c` — сгенерированный C для `try_read_u16_le`/`try_parse_int`
> (ДО правок) уже даёт **конкретный** `NovaRes_uint16_t_Nova_ReadBufferError_p*` /
> `NovaRes_nova_int_Nova_ParseIntError_p*` на call-site (Channel-2 через
> `infer_method_call_channel_type`→`resolve_instance_method_return`), а НЕ erased-хардкод
> `NovaRes_nova_int_nova_str*`. Хардкоды были **dead-superseded** каналом (Ф.2c/172.1.2-механизм).
>
> **ЧТО СДЕЛАНО:**
> - **parse.nv:** `@try_parse_int`→`@parse_int` (Result-форма), удалены bare `@parse_int`(throws) +
>   `@parse_int_opt`(Option). Result→Option на call-site = `.ok()`.
> - **read_buffer.nv:** `@try_read_X`→`@read_X` (22 Result-primaries), удалены **22 bare-twin**
>   Fail-обёртки (`_decode_utf8_at` не тронут).
> - **де-хардкод emit_c.rs (оба зеркала):** удалены `"parse_int" => "NovaOpt_nova_int"` +
>   ReadBuffer read_X/try_read_ width-таблица + `str_method_ret_type` parse_int-арм. Infallible
>   аксессоры (position/remaining/…) оставлены (вне D325-scope). NET: name-keyed return-хардкод
>   для parse/read СНЯТ; canal-miss → loud CC-FAIL (§1), не тихий mis-type.
> - **sweep call-sites:** parse (text_methods_test → `.ok()`; fe2 10 файлов try_→parse + git mv
>   `try_parse_int_*`→`parse_int_*`; fe5; unicode-golden parse-часть; convention переписан на D325
>   D85-каналы; cp_utils.nv), read (buffers/* bare→`!!` + try_→read; plan62; read_text; runtime).
> - **ГЕЙТЫ:** std компилится (nova-cli пересобран); negative-grep старых имён = 0 (кроме
>   rename-explain комментов); **zero-regression** vs baseline `19b9c756` (собран baseline-бинарь в
>   `nova-p177-base`): regression-сэмпл (generics/plan153_2/plan161/plan100_4_1) 3/1==3/1, broad
>   (plan59/72/148/138_2) 10/0==10/0, все affected-провалы доказаны **pre-existing** (overflow-neg
>   RUN-FAIL, golden-split HashMap CC-FAIL, auto_derive D249 — идентичны на baseline); pos-
>   conformance CU PASS. де-хардкод доказан: read_integers/read_oob/parse PASS с УДАЛЁННЫМИ
>   хардкодами.
>
> **ОТЛОЖЕНО (маркеры + backlog):**
> - **D77 4-way→2-way codegen** (`from_targets`/`try_from_targets` bare-throws-синтез в emit_c):
>   `[M-177-d77-codegen-4way-retract]` — сложно/отдельно (задевает всю conversion-auto-derive
>   машинерию), separable от parse/read; spec-часть уже внесена. Декларации TryFrom/TryInto не
>   тронуты. НЕ в этой задаче (task-разрешение).
> - **spec_tests/conformance/neg/d325_*** (removed-имена → compile-error): **БЛОКИРОВАНО
>   pre-existing чекер-багом** `[M-checker-unknown-method-stackoverflow]` — резолв неизвестного
>   метода на str/ReadBuffer-ресивере даёт **stack overflow** в чекере (воспроизводится на baseline
>   `19b9c756` тоже, НЕ регрессия) → чистый `EXPECT_COMPILE_ERROR`-фикстур невозможен (крашит
>   раннер). Negative-grep на уровне исходников доказывает отсутствие старых имён. Спец-неги —
>   после фикса stack-overflow (Ф.3).
> - **builtins `int.try_parse`/`f64.try_parse`/`char.try_from`** — цель Plan 174.1 (не Ф.2b).
> - **parse_int overflow-detection баг** (`[M-parse-int-overflow-returns-invaliddigit]`): 20-значные
>   числа → `Err(InvalidDigit)` вместо `Err(Overflow)` (neg `parse_int_overflow_err` RUN-FAIL,
>   **pre-existing** идентично на baseline; тело @parse_int byte-identical со старым @try_parse_int)
>   — арифметика parse.nv, домен Plan 174.1, вне D325-rename-scope.

> ⚠ **Line-refs ниже = снимок 2026-07-03 (исторический); Ф.2b выполнен по символу/паттерну.**

| Файл | Блокер | Sweep-скоуп (аудит 2026-07-03) |
|---|---|---|
| `std/runtime/string/parse.nv` (`try_parse_int`→`parse_int`) | `emit_c.rs` хардкодит C-тип возврата метода `parse_int` = `NovaOpt_nova_int` — **2 места** (паттерн `"parse_int"`; снимок ~:35241-35242 и ~:40362-40363). .nv-переименование без их правки → **silent mis-type** (Nova-body Result vs хардкод Option), не чистая ошибка. Удаления bare(:24) и `_opt`(:63) сами по себе .nv-only, но бессмысленны без rename. | **Defs:** parse.nv:24,35,63. **Call-sites вне тестов:** `std/unicode/cp_utils.nv` (1) + `std/STATUS.md` (док). **nova_tests: 98 hits / 15 файлов** — топ: `plan91/text_methods_test.nv` (26), `plan91_fe2/*` (58 в 12 файлах, вкл. neg/), `strings/plan91_18_parse_int_convention.nv` (6), `plan91_fe5` (5), `unicode/plan152_0_folder_split_golden.nv` (3). spec_tests: 0. |
| `std/runtime/read_buffer.nv` (`try_read_X`→`read_X`, удалить **22** bare-twin) | `emit_c.rs` хардкодит `read_X`→unboxed C-типы + `try_read_*`→Result (таблица; снимок ~:40012-40040, try-арм ~:40026-40030). Переименование без правки → mis-type на каждом call-site. | **22 пары** (byte, bytes, u8, i8, u16/i16/u32/i32/u64/i64 le+be, f32/f64 le+be, char, str; try_-primaries :66-421, bare-обёртки :428-449; приватный `_decode_utf8_at` — без пары, не трогать). **Call-sites: ~146 в 9 файлах nova_tests** — `buffers/*` 126 (read_integers 31, roundtrip 29, read_char_str 19, read_oob 14, read_floats 14, read_nav 12, write_floats 6, neg/neg_read_oob 1) + `runtime/read_buffer.nv` 20. ⚠ Ложные grep-совпадения (tcp_*, rwlock_try_write_for, read_text.nv, builder_chaining) — другие API, отфильтровать. |
| `std/prelude/protocols.nv` (ретракт bare auto-derive) | D77 4-way в `emit_c.rs` (`from_targets`/`try_from_targets`-синтез; снимок ~:675-678, ~:4184-4218). Декларации `TryFrom`/`TryInto` **не трогаем**. ⚠ Spec-часть D77-ретракта УЖЕ внесена 2026-07-01 (§3) — здесь только codegen. | — |
| builtins `int.try_parse`/`f64.try_parse`(→Result), `char.try_from` | `emit_c.rs` builtins (снимок: try_parse ~:28087-28110, try_from-арм ~:28206). Цель Plan 174.1 (+ баг truncation `i8.try_from("999")→-25`). | — |
| `std/testing/property.nv` — **4 неучтённых ранее** export-сигнатуры с `Fail` | `assert_prop`(:72), `assert_prop_msg`(:80) — собственный `throw PropertyFailed`; `property`(:345), `property_with`(:353) — mixed forwarded+own. Q5 ✅ решён 2026-07-03: **exempt** (assert/test-DSL) — в Ф.2b только вписать в guard-exempt (§8.2), НЕ мигрировать. | — |
| `std/_experimental/encoding/hex.nv` | механически .nv-only, но `_experimental` → отложить (§9 Q3). | — |

### ✅ Уже conformant (без изменений)
`std/net/*`, Plan 176, `std/encoding/utf16.nv` (`from_utf16 -> Result` — эталон целевой формы), `std/runtime/string/core.nv` (`try_from_codepoint` уже Result; `from_bytes_*` намеренно инфаллибл — не fallible, не трогать). `examples/effect_density/http.nv` и `examples/real_world/oxsar_port.nv` — user-code style `Fail` (легально под D325), std-triaду не зовут → **вне blast-radius**, правок не нужно. Дрейф-чек 2026-07-03: 0 новых bare-throw API в stable std после 2026-06-25.

### Отложенный `_experimental` (§9 Q3) — **полный список (Ф.4-аудит 2026-07-04)**
**17 файлов** ещё throw own-`Fail` (defer до стабилизации модуля; вне scope 177): `encoding/{csv,hex,ini,toml,url}.nv`, `data/{semver,semver_range,sql}.nv`, `crypto/{jwt,bcrypt}.nv`, `identifiers/{snowflake,ulid,uuid}.nv`, `math/statistics.nv`, `text/regex.nv`, `time/cron.nv`, `concurrency/retry.nv`. Прим.: `retry.@execute` внешний `Fail[E]` = forwarded (R5, легально даже после стабилизации); чинить только intrinsic `Db`/retry-ошибки. `sql.in_transaction` снят 2026-08-12, №570 (см. §14.2). **Маркер:** `[M-177-experimental-fallible-migration]` (см. §14.2).

### 🟡 Остаток stable-std вне scope 177 → Plan 173 (Ф.4-аудит 2026-07-04)
`std/concurrency/cancellation.nv` — `race2` (both-failed) и `with_timeout` (timeout) `throw` bare `str` через *inferred* Fail (сигнатура `-> T` без `Fail[` → guard §8.2 их не сканирует, §14.3). По R1 = expected failure → должны быть `Result`, НО structured-concurrency error-семантика = **Plan 173-домен** (§10/§13). **НЕ конвертировано** (нужен error-домен-тип + coordination 173 + смена 2 `#stable` сигнатур; whole-subsystem). **Маркер:** `[M-177-concurrency-throw-fallibility]` (home Plan 173; см. §14.1).

---

## 7. Фазы + DEP/гейты

| Фаза | Объём | DEP (что должно быть закрыто ДО) | Статус |
|---|---|---|---|
| **Ф.0 Discovery** | grep-скоуп: call-sites bare-`parse_int`/`read_X`/`_opt` + все `Fail[` в публичных std-сигнатурах (минус R5-forwarded) | — | ✅ DONE (workflow 2026-06-25; blast-radius-цифры уточнены аудитом Ред.2 2026-07-03 — §6) |
| **Ф.1 D325 + конвенции** | D325 в спеку + E1-E11 | sign-off (✅ 2026-06-25) | ✅ **DONE 2026-07-03** — E1-E11 все закрыты (E3/E6/E10/E11 добиты) + amend-пакет §4a внесён в D325 (R0/R4-критерий/nesting/exempt-list/коллекторы). Спека самосогласована с D325 (D178/D77 баннеры). |
| **Ф.2a `.nv`-only миграция** | base64 → complex → json | Ф.1-ядро | ✅ DONE 2026-06-26 (все 3 файла green end-to-end) |
| **Ф.2b compiler-gated sweep** | parse.nv + read_buffer.nv rename + emit_c хардкоды + builtins + D77-emit_c + sweep 15+9 тест-файлов (§6) | разрешение на компилятор; координация **172.1** (emit_c-зона) и **174.1** (новые поверхности сразу под D325) — Волна 3 | 🟢 **DONE 2026-07-04** (parse/read rename + де-хардкод return-типов §3/§10 + sweep; zero-regression vs `19b9c756`; де-хардкод доказан). **Отложено:** D77-codegen `[M-177-d77-codegen-4way-retract]`; builtins→174.1; spec-neg блок. на `[M-checker-unknown-method-stackoverflow]` (pre-existing); parse_int-overflow-баг (pre-existing, 174.1). |
| **Ф.2c std-коллекторы** | `sequence: []Result[T,E] -> Result[[]T,E]` (fail-fast) + `partition: []Result[T,E] -> ([]T,[]E)` в prelude (прецеденты: Rust `FromIterator for Result`, Go `errors.Join`); без них аргумент §1.3 — обещание, не API | ~~Ф.1; независим от Ф.2b (.nv-only)~~ ~~codegen-gated~~ | ✅ DONE 2026-07-04 (`sequence`+`partition` в `std/prelude/core.nv`, оба cross-module; codegen-блокер снят ЦЕЛЕВОЙ ФОРМОЙ `[M-172.1-U4-freefn-generic-return]` `3ef1acb8` — см. ниже) |
| **Ф.3 Guard + spec_tests** | conformance-guard (R5-дискриминатор + exempt-list §2) + нейм-линт (A2) + spec_tests d325/d77 (§8) | Ф.1 (полный D325-текст); neg-фикстуры на удалённые имена — после Ф.2b | ✅ **DONE 2026-07-04** — conformance `nova test --positive --compile-error spec_tests/conformance` = **PASS 41/0** (pos `d325_result_everywhere.nv` + 3 neg `d325_*`); guard+нейм-линт (`compiler-codegen/tests/d325_result_everywhere_guard.rs`, cargo, hermetic) = **3 passed, 0 нарушений в stable std** (own-Fail R1/R5 + `_opt`/orphan-`try_` R3/R4 + net-zero-Fail; exempt-list §2 явный + non-vacuity-ассерты); A1-A4 покрыты (A1 amend d13 R0/R1-контраст; A3 R5-forward HOF; A4 sequence/partition). Zero-regression by construction (0 правок compiler/std). **Отложено:** `d77_two_way_conversions` spec-fixture → `[M-177-d77-codegen-4way-retract]` (codegen-часть D77 не в scope Ф.3); `[M-172.1-opt-result-over-userenum-typedef-order]` (Option[Result[int,UserEnum]] typedef-ordering). |
| **Ф.4 Docs/log/закрытие** | `project-creation.txt` + `discussion-log.md` (nova-private) + `simplifications.md`; cross-ref из 174.1/173/176; Q-sweep §9; **аудит полноты D325 (§14)** | все предыдущие | ✅ **DONE 2026-07-04** — карта полноты D325 (§14): stable-std public-fallible = Result-everywhere; остаток честно маркирован (concurrency→173, `_experimental`→§9 Q3, codegen-хвост); spec D325 Status→migration-complete; README/backlog/simplifications/project-creation обновлены; conformance 41/0 (net-zero-код, docs-only). **План 177 ЗАКРЫТ.** |

**Гейт каждой фазы (Ред.2-канон):** spec_tests/conformance зелёный (d325 + amended d-файлы) + pos/neg-фикстуры фазы + **nova_tests baseline-delta = 0** (baseline = parent-коммит, ТОТ ЖЕ бинарь, temp-worktree/commit+reset; nova_tests сам по себе НЕ гейт корректности; флака ≠ регрессия). Для Ф.2b дополнительно: 0 вхождений старых имён по sweep-спискам §6 (negative grep).

> **✅ DONE 2026-07-04 — ЦЕЛЕВОЙ ФОРМОЙ (`[M-172.1-U4-freefn-generic-return]`, commit `3ef1acb8`;
> коллекторы `b8fb952d`):** codegen-блокер снят через ЕДИНЫЙ канал (D315), НЕ зеркало.
>
> **ИСТОРИЯ (2 захода):** Заход 1 (`4e4e7c34`) — codegen-side subst-mirror: Vec-flip `[]T` в
> `value_aware_subst_to_ref` (зеркаля `resolved_array_to_c`). Работал + zero-regression, НО был
> §10 «два окна правды» / §0-интерим на расходящемся пути (`apply_type_subst_to_ref` — static
> subst-mirror, ретайрится в пользу `resolved_type_to_c`). Плюс он чинил только Result/Option
> (sequence), а **tuple-over-array cross-module (`partition`) НЕ покрывал**: `pr.0` эрейзился в
> `int` через `fn_ret_<name>` hijack ДО зеркала. **Заход 2 (целевая форма, откатил зеркало
> net-zero):** нашёл истинный корень — §1 «резолвит, но discard». `f1_check_call` (checker)
> резолвит callee + знает args, но gate `!typeref_mentions_any(ret, callee_gs)` **ВЫБРАСЫВАЛ**
> generic-return (тот, что упоминает `T`/`E`) вместо субституции → канал `resolved_types` пуст →
> codegen Channel-2 miss → legacy (mirror для Result / `fn_ret` hijack для tuple).
>
> **ЧТО СДЕЛАНО (целевая форма):** в `f1_check_call` (`types/mod.rs` ~9384) вместо discard —
> ВЫВЕСТИ type-args из АРГУМЕНТОВ (структурный `crate::const_fn_trampoline::unify_type` каждого
> param-TypeRef против inferred-типа арг — тот же унификатор, что `build_recv_subst`) и
> СУБСТИТУИРОВАТЬ return → материализовать КОНКРЕТНЫЙ тип (`Result[[]int,str]` / `([]int,[]str)`)
> в `resolved_types`. Codegen читает его ЕДИНЫМ `resolved_type_to_c` (Channel-2, до legacy) →
> Vec-flip + mono-tuple из ОДНОГО источника. Гейты: FREE fn (no receiver — receiver-carrier =
> метод-канал `infer_method_call_channel_type`), FULLY resolved для ЭТОГО caller'а (residual
> type-param → skip → legacy; эрейзнутое generic-тело не трогаем). **Зеркало `4e4e7c34`
> УДАЛЕНО** (`value_aware_subst_to_ref` Array/Result/Option/Tuple-арм + `vec_flip_elem_c` +
> `typeref_contains_array`) — `emit_c.rs` NET-ZERO vs parent `12d30695`. Ни зеркала, ни дубля.
> **ГЕЙТ:** cross-module `sequence`+`partition` (`pr.0.len()`, `ro (a,b)=`, `pr.0.get()==Some`) +
> local `nova_tests/err177_collectors` PASS БЕЗ зеркала; conformance 38/38; **zero-regression** vs
> baseline `12d30695` на 23 папках вкл. tuple-heavy `plan59`/`plan148`/`plan115`/net(`plan91_12`):
> все SAME (pre-existing red идентичны — basics `apply`, plan172_showcase, plan114/net P67-panic,
> concurrency). Ниже — исходная (частично ошибочная) диагностика Заход-0, оставлена для контекста.
>
> <details><summary>Заход-1 диагностика (mirror, 4e4e7c34 — superseded)</summary>
>
> mono-СИГНАТУРА (fwd-decl) И ТЕЛО generic-fn на HEAD **уже корректны** — оба через
> `type_ref_to_c → resolved_array_to_c` Vec-flip'ят `[]T` в `Nova_Vec____nova_int*`. Расходилась
> **caller-side инференс типа вызова**: `value_aware_subst_to_ref → static apply_type_subst_to_ref`
> (Array-арм) выдавал pre-D239 raw-array `NovaArray_nova_int*` → `NovaRes_NovaArray_nova_int_p_nova_str*`
> (typedef никогда не эмитится → CC-FAIL); match-биндинги фолбечили на erased `Nova_T*`/`Nova_E*`.
> Заход-1 фиксил это зеркалом в `value_aware_subst_to_ref`. Заход-2 устранил и зеркало, и первопричину
> (checker discard) — см. выше. **zero-regression** заход-1 был на generics/basics/plan91/plan153_2/
> plan161/plan100_4_1/plan103_9/plan110/plan114/plan172*/plan138_2/plan145/concurrency/plan153_3 (все
> зелёные — зелёные;
> pre-existing red идентичны baseline).
>
> </details>
>
> <details><summary>Заход-0 диагностика (2026-07-03, гипотеза «mono-body-erasure» — ОКАЗАЛАСЬ ОШИБОЧНОЙ)</summary>
>
> **🔬 Находка 2026-07-03 (Ф.2c НЕ .nv-only — codegen-gated):** прототип коллекторов
> (`export fn[T,E] sequence(items []Result[T,E]) -> Result[[]T,E]` / `partition -> ([]T,[]E)`)
> **проходит чекер** (синтаксис/типы валидны — generic-free-fn + `[]T`-build + Result/tuple-payload
> подтверждены), но **падает в codegen (CC-FAIL)**: `unknown type name 'NovaRes_NovaArray_nova_int_p_nova_str'`
> (sequence) и `_NovaTuple_2_..._NovaArray_..._NovaArray_...` (partition). Это **VR-typedef-ordering баг
> для Result/Tuple над Array-payload при fresh mono-инстанциации** — wrapper-typedef эмитится ДО
> element-typedef. Подтверждение: `Result[[]u8,...]` из base64 (Ф.2a) РАБОТАЕТ (typedef эмитится под
> конкретный byte-инстанс: «did you mean nova_byte_p_nova_str»), а `Result[[]int,str]`/tuple — нет.
> **Тот же класс, что `[M-181-result-over-named-tuple-codegen]` (b022919a, Ф.2a) — но для ARRAY-
> payload.** **УТОЧНЁННЫЙ КОРЕНЬ (после попытки фикса 2026-07-03):** это НЕ просто late-VR-typedef
> ordering, а **неполная mono-подстановка для формы `fn[T,E] … -> Result[[]T,E]`**: тело generic-mono
> инстанса эмитится с **эрейзнутым `E` (`Nova_E*` вместо `nova_str`)** — `Err(e) => return Err(e)` даёт
> `initializing 'Nova_E*' with 'nova_str'` — и **конкретный `NovaRes_NovaArray_nova_int_p_nova_str` typedef
> НЕ регистрируется** (caller юзает его, он нигде не определён; byte-версия есть только потому, что base64
> её регистрирует конкретным `Ok`). **Попытка localized-фикса** (idempotent-регистрация NovaRes возврата
> на входе emit-fn, зеркало ctor-site 23631) **НЕ помогла** — `ret_c` для generic-mono не конкретен, корень
> глубже (mono-substitution ядра, зона 172.1: `M-172.1-*`/`M-181-*` markers по всему mono-typedef-пути).
> **Координация СНЯТА (2026-07-03):** `plan-172` = **21 коммит ПОЗАДИ main** (0 ahead) → вся 172.1-работа
> **уже в main**; nova-p177/p173 (из main) её содержат, а баг ВСЁ РАВНО воспроизводится → это **genuinely
> OPEN баг в текущем main**, НЕ «активная 172.1-зона». Блокер — не координация, а **глубина+regression-risk**.
> **2 localized-фикса ПРОВАЛИЛИСЬ (эмпирически, с пересборкой):** (1) register NovaRes на входе emit-fn
> METHOD-пути (~15767) — не сработал (free-fn `seq_a` идёт не туда); (2) то же в FREE-пути
> `register_mono_instance` (~15386) — не сработал И **внёс регрессию `Nova_E*`** (было 1 error, стало
> больше). Оба откачены (net-zero emit_c). **Подтверждённый корень:** mono-ТЕЛО `fn[T[,E]] -> Result[[]T,E]`
> эмитится с эрейзнутыми типами (`Nova_E*`, generic array-элемент) — worklist body-drain НЕ пере-применяет
> type-subst для этой формы. Это **foundational mono-instantiation** (не typedef-registration), high-regression-
> risk (задевает ВСЕ generic-fn), нужен full-regression-гейт. Маркер **`[M-177-result-tuple-over-array-codegen]`**.
> Чистого .nv-workaround для `Result[[]T,E]` нет. **ТОЧНЫЙ КОРЕНЬ (найден трассировкой):**
> `resolved_type_to_c` Result-арм (`emit_c.rs:2232-2251`): если Ok-payload `[]T`'s **ResolvedType** держит
> `T` generic (`is_generic_stub_c`=true), весь Result **эрейзится в fallback `NovaRes_nova_int_nova_str*`**
> (:2251) → тело идёт по `__erased__`-worklist-пути (:4561 → `emit_generic_fn_erased`, отсюда `Nova_E*`).
> А СИГНАТУРА через `type_ref_to_c`+`current_type_subst` даёт КОНКРЕТНЫЙ `NovaRes_NovaArray_nova_int_p_nova_str*`
> (его юзает caller) → mismatch + неопределённый typedef. **Это `TypeRef`(subst-aware) vs `ResolvedType`
> (НЕ-subst-aware в этой точке) дуальность** — ровно проблема, которую решает **Plan 172 (unified type
> engine)**. Фикс = align mono-subst через оба type-представления (или resolved-subst для `[]T` в mono-теле)
> — **foundational, домен Plan 172**, не solo-патч в 177. **Итог:** Ф.2c зависит от 172-type-engine-unification
> (либо dedicated фикс `resolved_type_to_c` Result-арма с subst-протяжкой + broad regression).
>
> **[РЕТРОСПЕКТИВА]** Гипотеза «mono-body-erasure» (тело эмитится с `Nova_E*`) была НЕВЕРНА для HEAD:
> тело+сигнатура уже корректны, `Nova_E*` был в CALLER-e. Истинный корень — checker-discard
> generic-return (см. Заход-2 выше). Оставлено как урок: диагностировать по СГЕНЕРИРОВАННОМУ C, не по симптому.
>
> </details>

---

## 8. Тесты / guards

### 8.1 spec_tests/conformance — ОБЯЗАТЕЛЬНОЕ D-покрытие (Ред.2-канон) — ✅ DONE 2026-07-04

- ✅ **`spec_tests/conformance/d325_result_everywhere.nv`** (часть CU) — pos: Result-форма (`"42".parse_int()` → `Ok`, `rb.read_u32_le()`), все каналы D85 (`!!` → значение, `?`-проброс, `.ok()` → Option, `match`); R5-positive (**A3**): HOF `d325_forward` прокидывает `Fail[E]` из closure-параметра (turbofish `[T,E]` — `E` только в эффект-позиции; typed lambda-handler `with Fail[E] = |e| …`); коллекторы (**A4**): `sequence` = `Err(первый)` / `partition` = `([1,2],[e])` над `Result[int,str]` (parse_int + domain-str `match`-канал — прямой `Result[int,ParseIntError]` упирается в codegen-баг `[M-172.1-opt-result-over-userenum-typedef-order]`).
- ✅ **`spec_tests/conformance/neg/d325_*.nv`**: `try_parse_int` (str) + `parse_int_opt` (str) → `[E_UNKNOWN_METHOD]` (primitive-receiver — новый канал, разблокирован `d0208346`); `try_read_u32_le` (ReadBuffer = user-struct) → `[E7320] no field or method` (существующий struct-канал). Формат: standalone `module neg.<имя>`, маркер `// EXPECT_COMPILE_ERROR <substr>` **без двоеточия**.
- ⏳ **Amend d77-файла** (`d77_two_way_conversions.nv`, bare `T.from` fallible НЕ генерится / `try_from` → Result) — **отложено**: codegen-часть D77 4-way→2-way = `[M-177-d77-codegen-4way-retract]` (не в scope Ф.3; spec-часть уже внесена §3/E11). Нейм-линт покрывает R3-сторону (`try_from` требует infallible `from`).
- ✅ **R0/D13-граница (A1):** amend `d13_with_fail_recoverable_vs_panic.nv` — recoverable-сторона (`v.get(oob)` → `None`, `"x".parse_int().ok()` → `None`) проверена позитивно; panic-категория (`v[oob]`, overflow) не съедена R1 — cross-ref на существующие `EXPECT_RUNTIME_PANIC` фикстуры (amend, не дублируем; runtime div0 clean-panic отсутствует — UB без guard, фикстуру не добавляли).
- Прогон: `nova test --positive --compile-error spec_tests/conformance` → **PASS 41 / FAIL 0**.

### 8.2 Guards / линты — ✅ DONE 2026-07-04 (`compiler-codegen/tests/d325_result_everywhere_guard.rs`, cargo, 3 passed)

- ✅ **Conformance-guard** (`guard_no_own_fail_in_public_std_signatures`): скан исходника `std/**.nv` (hermetic) — публичная сигнатура с `Fail[E]` для собственной ошибки = FAIL; `Fail[E]` из `fn(…) … Fail[E]`-параметра (R5 forwarding) = OK (name-set дискриминатор param-регион vs return-регион). **exempt-list из §2** (Option@unwrap/Result@unwrap, on_exit, property.nv по Q5) — явным списком. `_experimental` исключён (§9 Q3). **0 нарушений в stable**; non-vacuity-ассерт `exempted_own_fail ≥ 3`.
- ✅ **Нейм-линт (A2)** (`naming_lint_no_opt_suffix_or_orphan_try_prefix`): FAIL на суффикс `_opt$` (0 в stable) и на `try_`-префикс **без** infallible-сиблинга (progressive base-strip, global set — `from`/`into` = builtin D73/D77; exempt `try_start`/`try_start_won` = Once non-blocking init, R4-absence). non-vacuity `try_seen ≥ 8`.
- ✅ **Net regression** (`net_family_has_zero_fail`): `std/net/**` = 0 `Fail[` в публичных сигнатурах.

### 8.3 Behavior (nova_tests; Ф.2b переписывает существующие фикстуры in-place — §6 sweep-списки, новых папок не требуется)

- `parse_int("x")` → `Err(...)`; `parse_int("42")!!` → `42`; `parse_int("42").ok()` → `Some(42)`.
- **Negative:** старые имена `try_parse_int`/`parse_int_opt`/bare-`read_X` → `E_UNKNOWN_METHOD` (spec_tests neg/ + negative grep по sweep-спискам).
- Err-Result-проверки — **позитивные** test-блоки с assert на Result (НЕ neg/; neg/ = только compile-error) — Ред.2-канон 175 §7/176 §7.

---

## 9. Открытые вопросы — РЕЗОЛЮЦИИ (аудит 2026-07-03)

- **Q1 — нейминг-правило §2 (R2/R3).** ✅ **ЗАКРЫТ**: sign-off владельца 2026-06-25, D325 ACTIVE в спеке.
- **Q2 — governance (live vs staged).** ✅ **ЗАКРЫТ де-факто**: E-правки внесены live (E1/E2/E4/E5/E7 уже в доках); остатки — §5.
- **Q3 — `_experimental`.** ✅ **ЗАКРЫТ**: отложить с TODO (выбрано ранее, подтверждено).
- **Q4 — Plan 174.1.** ✅ **ЗАКРЫТ**: 177 задаёт D325, 174.1 реализует per-type; 174.1 Ред.2 полностью переписан под D325 (174.1:30,37-45,90,108-141) — синхронизация выполнена.
- **Q5 — `std/testing/property.nv`.** ✅ **ЗАКРЫТ (sign-off 2026-07-03): exempt** — тестовый DSL (assert-семантика, аналог `assert` самого языка); 4 сигнатуры (`assert_prop`:72/`assert_prop_msg`:80/`property`:345/`property_with`:353) вписаны в exempt-list §2 + guard §8.2. Миграция в Result отвергнута (шум в тестах).

---

## 10. Координация

- **Plan 174.1** (primitive parse) — §9 Q4 ✅. **177** = правило (D325); **174.1** = per-type реализация под Result.
- **Plan 172.3** (type-set bounds) — ортогонально (схлопывает per-type обёртки в generic; нейминг общий из D325).
- **Plan 172.1** — Ф.2b трогает `emit_c.rs` = активная зона 172.1: НЕ пересекать коммиты; line-refs дрейфуют ежедневно → искать по символам (§6).
- **Plan 173** (error-machinery: defer-kernel/MultiError/structured-concurrency) — нейминга не касается; **177** лишь снимает stale net-pointer (E8 ✅ moot). 173 Ф.1 делает `?` строго return-only (`[E_TRY_IN_FAIL_FN]`) — примеры/конвенции 177 уже совместимы. `Fail`-эффект, `!!`/`?` — общие, не трогаем. Коллекторы Ф.2c ортогональны MultiError (173): `sequence`/`partition` — чистые списковые операции, MultiError-агрегация остаётся за 173.
- **Plan 176** (io/fs/os) — уже Result; **177** фиксирует, что он conformant; снять формулировки «по net-семейству как исключение». **Edge R4-`env`** (non-unicode на Windows → Result vs lossy-гарантия) — решение за 176, зафиксировать при его Ф.3-os.

## 11. Агент-правила (обязательны при исполнении; Ред.2-канон)

- **Git:** НЕ `git stash` (shared `.git`, конкурентные worktree); baseline = temp-worktree (`git worktree add ../nova-177-base <parent>`) либо commit+reset. `git add` только по именам файлов (никогда `-A`/`.`); перед commit — `git diff --cached --stat` (чужие pre-staged). DCO: `git commit -s`. БЕЗ `Co-Authored-By`. Коммит на фазу/задачу; после фазы — bidirectional sync с main.
- **Идемпотентность (rate-limit):** commit-per-phase, no-amend, null-tolerant агрегация — падение агента не должно терять работу.
- **Worktree:** постоянный `nova-p177` (naming nova-pNN); самозарегистрироваться первой командой; cwd дрейфует → каждую git/cargo-команду абсолютным путём или `git -C`. Env: `NOVA_GC_LIB_DIR`/`NOVA_GC_INCLUDE_DIR` → main-repo; libuv-submodule скопировать и удалить его `.git`.
- **Build:** Ф.2b — **mtime-touch `.rs`** перед `cargo build` (stale-build риск); после правок std `.nv` — пересобрать nova-cli (prelude вшивается).
- **Тесты:** `nova test` требует **ЯВНЫЙ путь**. Батч-канон: `nova test nova_tests/<dirs> --results-file rN.json` батчами <10 мин + хвост `--rerun-failed`; **ОТДЕЛЬНО** `nova test spec_tests` и `nova test std`. Гейт = §7 (spec_tests + pos/neg + baseline-delta=0, тот же бинарь; флака ≠ регрессия). Per-fix verify = targeted fixture, полный прогон — в конце фазы.
- **Не выдумывать синтаксис Nova** — сверяться со spec/decisions/ + examples/.
- Подтверждение владельца перед фоновыми агентами.

## 12. Критерии приёмки (сводные)

1. **Без упрощений, как для прода** — обязательный критерий: ни одного «пока так»; всё несделанное — только явный followup-маркер/делегация с владельцем.
2. D325 + **amend-пакет §4a** (R0/R4-критерий/nesting/exempt/коллекторы) committed в `04-effects.md`; E1-E11 все ✅ (включая E10 retract-баннер D178 — спека нигде не противоречит D325).
3. `spec_tests/conformance` d325 (pos+neg) + d77 зелёные; A1-A4 покрыты.
4. Conformance-guard + нейм-линт: 0 нарушений в stable std при **явном** exempt-list (§2); `_experimental` — только маркированные TODO.
5. Ф.2b: 0 вхождений старых имён по sweep-спискам §6 (negative grep 15+9+2 файлов); 22 bare-twins удалены; emit_c-хардкоды сняты по символам; **baseline-delta = 0**.
6. Ф.2c: `sequence`/`partition` в prelude + позитивный тест (§8.1 A4).
7. Q1-Q5 закрыты с записанными резолюциями; docs/log обновлены (Ф.4).

## 13. Не в scope

- Реализация Fs/Io/Os (**Plan 176**); defer-kernel/MultiError (**Plan 173**); операторы `!!`/`?`/`??` (**D85**, стабильны).
- **Удаление эффекта `Fail` из языка** — НЕ делаем; он остаётся для пользовательского кода и внутренних хелперов.
- Стабилизация `_experimental` сверх fallible-контракта.

---

## 14. Аудит полноты D325 (Ф.4 close-out, 2026-07-04) — честная карта

> **Метод.** `grep 'Fail\['` + `grep '\bthrow\b'` по всей `std/**.nv`, фильтр не-комментов, разбор сигнатур
> vs тело. Cross-check с conformance-guard (`compiler-codegen/tests/d325_result_everywhere_guard.rs`, 3
> passed, 0 нарушений в stable) и conformance CU (41/0). **Вывод: D325-конвенция достигнута полностью для
> in-scope stable-std; остаток вне scope честно маркирован — не выдаём частичное за полное (§12.1, §7.7).**

### 14.1 Stable std (не-`_experimental`) — public fallible ops

| Категория | Файлы / API | Статус D325 |
|---|---|---|
| **Мигрировано (Ф.2a/2b/2c)** | `encoding/base64.nv` (decode* → `Result[[]u8, Base64Error]`); `encoding/json.nv` (parse/Parser/Lexer → `Result[…, ParseJsonError]`); `runtime/string/parse.nv` (`parse_int → Result[int, ParseIntError]`, bare+`_opt` удалены); `runtime/read_buffer.nv` (22 `read_X → Result[…, ReadBufferError]`, 22 bare-twin удалены); `prelude/core.nv` (коллекторы `sequence`/`partition`); `_experimental/math/complex.nv` (`try_from → Result`) | ✅ **Result-everywhere** |
| **Уже conformant (pre-177)** | `net/*` (0 `Fail[`, эталон); Plan 176 io/fs/os (Result by-design); `encoding/utf16.nv` (`from_utf16 → Result`); `runtime/string/core.nv` (`try_from_codepoint → Result`; `from_bytes_*` намеренно инфаллибл) | ✅ conformant |
| **Exempt-list §2 (by-design — несут `Fail[`, легально)** | `prelude/core.nv` `Option@unwrap`/`Result@unwrap` (мост D85 `!!`); `prelude/protocols.nv` `on_exit`/`@cleanup` (R5 forwarding, user-`E`); `prelude/effects.nv` `type Fail[E] effect` (сам механизм); `testing/property.nv` `assert_prop`/`assert_prop_msg`/`property`/`property_with` (Q5 test-DSL) | ✅ exempt (guard non-vacuity `≥3`) |
| **🟡 ОСТАТОК (маркирован)** | `concurrency/cancellation.nv` — `race2` (both-failed) и `with_timeout` (timeout) **throw bare `str`** через *inferred* Fail (в сигнатуре `-> T` НЕТ `Fail[` → **вне guard-скана**, §14.3). По R1 timeout/both-failed = expected failure → должны быть `Result`. НО structured-concurrency error-семантика = **Plan 173-домен** (§10/§13, вне scope 177): нужен error-домен-тип + coordination с MultiError/173 Ф.4 + смена 2 `#stable` сигнатур. **НЕ конвертировано** (не тривиально; whole-subsystem → маркер). | ⏳ `[M-177-concurrency-throw-fallibility]` (home Plan 173) |

### 14.2 `std/_experimental/**` — полный остаток (defer §9 Q3)

Все fallible-API `_experimental` **ещё throw own-`Fail`** (не Result). Отложено до **стабилизации каждого модуля**
(§9 Q3 — pre-prod поверхность, вне scope 177). **17 файлов:**
`encoding/{csv,hex,ini,toml,url}.nv`, `data/{semver,semver_range,sql}.nv`, `crypto/{jwt,bcrypt}.nv`,
`identifiers/{snowflake,ulid,uuid}.nv`, `math/statistics.nv`, `text/regex.nv`, `time/cron.nv`,
`concurrency/retry.nv`. **NB:** `retry.@execute` несёт `Fail[E]` **forwarded** из
closure-параметра (R5, легально даже после стабилизации — чинить только intrinsic-ошибки retry).
`sql.in_transaction` **снят 2026-08-12, №570** (owner decision, реестр 221.1: generic-операция
эффекта — named checker refusal, Q6; носитель убран из `Db` окном `p570-generic-refusal`, не
переписан — замена спроектирует план 268, `consume`-тип `Tx` + `@cleanup`, план 273 §4-тер).
**Маркер:** `[M-177-experimental-fallible-migration]` (консолидирует бывш. §6-список sql/jwt/snowflake/ulid/bcrypt/retry — теперь полный).

### 14.3 Известное ограничение guard'а (честно)

Conformance-guard §8.2 сканирует **литерал `Fail[`** в return/effect-регионе сигнатуры. Функция, которая
`throw`-ит через **inferred** Fail-эффект (сигнатура написана `-> T` без `Fail[`), guard'ом **не ловится**
(`race2`/`with_timeout` — единственные такие в stable). Это by-design для дешёвого hermetic-скана источника;
throw-based fallibility ловится отдельным аудитом (§14, grep `\bthrow\b`). Усиление guard'а до эффект-инференса
= компилятор-в-тесте (дорого) — не делаем; §14.1-остаток покрывает пробел явным маркером.

### 14.4 Прочие OPEN-маркеры 177 (codegen-хвост, уже зарегистрированы)

- `[M-177-d77-codegen-4way-retract]` — D77 4-way→2-way **codegen** (spec-часть внесена; `from_targets`-синтез в `emit_c.rs` отложен, separable, own regression-гейт).
- `[M-172.1-opt-result-over-userenum-typedef-order]` — `Option[Result[int, UserEnum]]` typedef-ordering в теле generic-коллектора (домен Plan 172.1 mono-typedef-registration).
- `[M-parse-int-overflow-returns-invaliddigit]` — pre-existing overflow-детект `parse_int` (домен Plan 174.1, вне D325-rename-scope).

**Итог:** D325 (правило + spec + guard + conformance + миграция in-scope stable-std) — **закрыт полностью**.
Остаток = (a) 2 concurrency-throw'а (Plan 173), (b) `_experimental` (стабилизация/§9 Q3), (c) 3 codegen/арифметика-маркера — **все с явным home**. **План 177 ЗАКРЫТ.**
