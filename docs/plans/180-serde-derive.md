<!-- SPDX-License-Identifier: CC-BY-4.0 -->
# Plan 180 — std/encoding/serde (Serialize/Deserialize + компиляторный auto-derive)

> **Уровень:** Top-level.
> **Создан:** 2026-06-26. **Статус:** `proposed`.
> **Маркер:** `[M-180-serde-derive]`.
> **Запуск:** «выполни план 180».
> **Эталон:** Rust **serde** (GOLD STANDARD — data-model, Serializer/Deserializer/Visitor, enum-tagging-matrix, attribute-набор) + Swift **Codable** (ближайший к Nova: compiler-synth, container-абстракция, `CodingKeys`) + Kotlin kotlinx.serialization (plugin-synth, format-pluggability) + zod (path-rich validation-ошибки). Анти-эталон: Go `encoding/json` / Java Jackson (runtime-рефлексия, silent tag-typo, JSON-привязка).
> **D-блоки (NEW):** D340–D346 (data-model+протоколы / record-auto-derive-контракт / data-model↔synth-mapping / атрибуты+валидация / JSON-backend / enum-tagging / numeric+depth-soundness). **D-high-water = D332 (Plan 178).** Старт **D340** (gap D333–D339 зарезервированы под Plan 179 compress + резерв — фиксируется как Plan 177 §«зарезервированы»; verify/renumber в Ф.0).
> **HARD-PREREQ / GATES (честно, см. §4 Ф.0):**
> - 🔴 **`[M-126-sum-equal-rich]`/`-clone-rich`/`-hash-rich`** (sum rich auto-derive) — **OPEN на main.** Sum-auto-derive serde (Ф.2-sum) **ГЕЙТИТСЯ на закрытии этого семейства**, НЕ переиспользует несуществующую машину. Ф.2 «сейчас» = **RECORD-only**; sum-auto-derive + enum-tagging → **отдельный гейт/под-план 180.2** (см. ниже).
> - 🔴 **`[M-161-parametric-return]`** — OPEN на main (blanket-dispatch V1 = только конкретный return-тип; `fn[T Serialize] Option[T] @serialize` рекурсия в element — НЕ покрыта). Container/Option/generic-conformance (`[]T`/`Option[T]`/`HashMap`) **НЕ может быть blanket-`.nv`** сегодня → реализуется как **компиляторные monomorphic special-cases в синтезаторе** (Ф.2), НЕ как `.nv` blanket-impl. Verify+решение в Ф.0.
> - 🟡 **Attribute-инфра** — `#serde(k=v)` на полях/вариантах **не существует** (AST `RecordField`/`SumVariant` без `attrs`; парсер знает только hardcoded type-attrs + field-level `#visible_to`). Ф.3 расщеплён на **Ф.3a (parser+AST+validation)** → **Ф.3b (synth-consumption)**. Field-attr-прецедент = `#visible_to` (parser/mod.rs:4315) — расширяем, не с нуля.
> **DEPS:** Plan 177 (D325, Result-everywhere — ✅ landed); `std/encoding/json` (✅ production, RFC 8259); `std/encoding/base64` (для `bytes`↔base64); auto-derive-машина `#impl(P)` (D109 amend + D230, ✅ landed для record/tuple — [auto-derive-guide.md](../auto-derive-guide.md)).
> **Координация:** 🔓 **этот план разблокирует Plan 178 (std/http)** typed `.json[T]()`/`.json(v T)` (178 Q20 HARD-GATE, [178:9/217/428](178-std-http.md)) — для **record-DTO** (Ф.4, record-only Ф.2). Dynamic `.json() -> JsonValue` в 178 приземляется СЕЙЧАС над существующим json (не ждёт serde).
> **Сквозной критерий (§8.0):** ОБЯЗАТЕЛЬНО без упрощений, как для прода. Auto-derive (record) + JSON-backend — критический путь; ни одного «решим потом» на нём; sum/attrs/depth/numeric — каждое либо приземлено с pos+neg+звучностью, либо честно GATE/scope-out с обоснованием.

---

## 1. Зачем

В Nova **есть `std/encoding/json`** ([json.nv](../../std/encoding/json.nv): `JsonValue` sum-type, `Json.parse(s) -> Result[JsonValue, ParseJsonError]`, `v.@into() -> str`, `pretty`, RFC 8259, round-trip-tested — ✅ полностью зелёный 2026-06-26 после Plan 177 Ф.2a) — но это **dynamic-value-слой**: пользователь работает с `JsonValue.Object(HashMap[str, JsonValue])` и руками `match`-ит/`as_object()?.get("name")?.as_str()?` каждое поле. **Нет typed-моста** `record/sum <-> wire-format` — нет способа сказать «возьми МОЙ тип `User { name str, age int }` и (де)сериализуй его». Это держит Nova на уровне «ручной разбор дерева»: ни одного first-party языка backend-ниши без typed-serde не существует (Rust `serde`, Swift `Codable`, Kotlin `kotlinx.serialization`, Go `encoding/json`-теги, Java Jackson, TS zod) — это **базовая инфраструктура** backend/API-направления Nova (Plan 18: web/backend для 0.2).

**🔴 Прямой HARD-GATE — этот план разблокирует Plan 178.** [178:9-10](178-std-http.md) явно гейтит **typed `.json[T]()`/`.json(v T)`** на «**NEW serde-sub-plan** (auto-derive `Serialize`/`Deserialize`; owner 2026-06-26)»; dynamic `.json() -> JsonValue` Plan 178 приземляет сам над существующим `std/encoding/json`, но typed-форма — то, ради чего HTTP-клиент эргономичен (`client.get(url).send()?.json[User]()` вместо ручного дерева) — **не может приземлиться без Ф.4 этого плана**. То есть Plan 180 — на критическом пути std/http (как `std/encoding/compress` для auto-decompress, [178:9](178-std-http.md)). **Важно (честно):** typed `.json[User]` для HTTP нужен только для **record-DTO** (request/response — это структуры, не sum-типы с payload), а record-auto-derive приземляется в Ф.2 «сейчас». Sum-auto-derive + enum-tagging (Ф.2-sum/Ф.5) **НЕ на критическом пути 182** — гейт открывается record-only.

**Что это разблокирует помимо HTTP:** (1) **typed API-клиенты/серверы** — request/response-DTO как обычные record, `json.decode[Req](body)?` на входе, `json.encode(resp)?` на выходе, без ручного `JsonValue`-дерева; (2) **конфиги/манифесты** — `nova`-tooling сам читает package-манифесты (Plan 03.x), CI-конфиги в typed-структуры с валидацией-по-типу; (3) **формат-агностичность** — один раз (де)сериализуемый тип работает поверх JSON **сейчас** и TOML/binary/MessagePack **позже** без переписывания (§11), потому что протокол `Serialize`/`Deserialize` отделён от backend'а (`JsonSerializer`); (4) **RPC/IPC/persistence** — обмен сообщениями, snapshot'ы состояния, wire-протоколы поверх единого typed-слоя. Всё **fallible → `Result[T, SerError]`/`Result[T, DeError]`** (D325), backend **pure** (без I/O-эффекта — кодек над байтами/значениями, не triad).

## 1a. Где Nova ЛУЧШЕ peers (differentiators — в доку)

- **🏆 AUTO-DERIVE через ЕДИНЫЙ `#impl`-mechanism — headline-win. Компилятор синтезирует `Serialize`/`Deserialize` той же memberwise-рекурсивной машиной, что уже авто-выводит `==`/`clone`/`hash`/`compare`/`display`** ([auto-derive-guide.md](../auto-derive-guide.md): D109 amend + D230, memberwise рекурсивно по полям). Serde становится **седьмым/восьмым членом** этого семейства — никакого `#[derive(Serialize)]`-обвеса как у Rust (там это отдельный proc-macro-плагин), никакого `: Codable`-conformance с ручным `init(from:)` для enum'ов (Swift), никакой runtime-рефлексии по тегам (Go/Java). **Решение по UX (§3.0 Q1, честно):** Nova использует **тот же `#impl(Serialize + Deserialize)` opt-in**, что и `#impl(Equal+Hash+Clone)` — НЕ «магический default-on» (это сломало бы консистентность семейства auto-derive, где `==`/`clone` тоже требуют `#impl`). **Differentiator не в «нулевой аннотации», а в «нулевой церемонии тела»:** одна и та же `#impl`-аннотация даёт и `==`, и serde **бесплатно по структуре**, тогда как Rust требует ОТДЕЛЬНЫЙ `#[derive(Serialize, Deserialize)]` + crate, Swift — conformance + ручной код для enum, Go — медленную рефлексию. «Структура типа — её serde-контракт» через уже-существующий, уже-знакомый пользователю механизм.
- **🏆 compile-time, zero-cost, type-safe — НЕТ runtime-рефлексии (бьёт Go/Java насмерть).** Тело (де)сериализатора эмитится в момент компиляции (как `@equal`), мономорфизировано под конкретный тип → **нулевая reflection-стоимость в рантайме** и **невозможна ошибка-по-строке**: опечатка в имени поля/теге у Go (`json:"naem"`) и Jackson (`@JsonProperty("naem")`) **компилируется** и молча теряет данные; в Nova поле-routing статический, опечатка ключа в `#serde(rename=…)` → **compile-error** `E_SERDE_BAD_ATTRIBUTE`. Type-mismatch (JSON `"42"` в `int`-поле) → **typed `DeError` с path**, не паника и не silent-coerce.
- **🏆 format-agnostic data-model (как serde, но lean для Nova).** Протокол `Serialize`/`Deserialize` говорит на **format-agnostic data-model** (`bool/int/uint/float/str/bytes/option/seq/map/struct/enum/unit`), а не на JSON напрямую → **один и тот же derived-тип** едет в JSON **сейчас** и TOML/binary **позже** (§11) **без перекомпиляции типа**. Go `encoding/json` зашит в JSON; Swift `Codable` агностичен (✅ паритет), но требует conformance + ручной enum-код; Nova = агностичность serde + единый `#impl`-механизм **одновременно**.
- **🏆 typed `DeError`/`SerError` с PATH/LOCATION (D325, R1).** Единый структурный `DeError{kind, path, location, source}` / `SerError{kind, path}` с OPEN `ErrorKind` и **JSON-path к месту ошибки** (`$.users[3].age: expected int, found string (line 5, col 12)`) — диагностика уровня serde+zod, недостижимая для Go (`json: cannot unmarshal …` без полного пути в nested-массивах). Source-chain в `ParseJsonError` (line/col не теряются).
- **byte-first + Result-everywhere, backend = PURE (без эффекта).** Serde-слой **не несёт I/O-эффект** (это кодек над `[]u8`/значениями, не triad — как чистый base64/utf16, не как net): `json.encode[T](v) -> Result[str, SerError]`, `json.decode[T](s) -> Result[T, DeError]`, плюс байт-формы `decode_bytes` поверх `[]u8`. Всё fallible → `Result` (D325/R1), `Option` только для genuine-absence полей (R4), `Fail[E]` для собственных ошибок запрещён (R5). Бьёт Node (silent lossy decode) и Go (`Unmarshal` в `interface{}` теряет типы).
- **leverage существующего `std/encoding/json` (не дублируем парсер).** JSON-backend `JsonSerializer`/`JsonDeserializer` строится **поверх** уже-production `Json.parse`/`@into`/`JsonValue` ([json.nv](../../std/encoding/json.nv)) — Ф.4 это **тонкий мост data-model ↔ `JsonValue`**, а не новый парсер. Reuse RFC-8259-логики, surrogate-pairs, escape, `format_num`-2^53-инварианта, strict-`DuplicateKey`.
- **🟡 enum-tagging full matrix — differentiator «позже» (честно).** Externally-tagged по умолчанию (`{"Variant": payload}`), `#serde(tag="type")` internal, adjacent, untagged — на **родных** Nova sum-типах с structural-eq (Plan 172.1). Swift `Codable` enum-with-associated-values требует **ручного `init(from:)`**; Go вообще не имеет sum-типов. **НО:** sum-auto-derive синтез **на main НЕ существует** (`[M-126-sum-*-rich]` OPEN) → это **гейт/под-план 180.2**, НЕ «бесплатный reuse». Заявляем differentiator честно как **планируемый**, гейтнутый на sum-synth-инфру.

## 2. Эталон (cross-lang serde/derive)

**🏆** = Nova **строго лучше** лучшего peer'а; **=** = паритет; **🟡** = таргет-differentiator, гейтнут (sum-synth/later).

| Фича | **Nova-target** | Rust serde | Swift Codable | Kotlin kotlinx | Go encoding/json | Java Jackson/Gson | TS zod/io-ts |
|---|---|---|---|---|---|---|---|
| **derive-mechanism** | **🏆 единый `#impl(Serialize+Deserialize)` (та же машина, что `==`/`clone`/`hash`; нет отд. плагина/conformance)** | `#[derive(Serialize, Deserialize)]` (отд. proc-macro crate) | compiler-synth, но `: Codable` conformance + ручной `init(from:)` для enum | compiler-plugin `@Serializable` | **runtime reflection** + struct-теги (no derive) | **runtime reflection** + аннотации | runtime schema-объект (ручной `z.object`) |
| **runtime-cost** | **🏆 zero (monomorphized compile-time, как `@equal`)** | zero (monomorphized) = | low (synth, protocol-dispatch) | low (plugin-generated) | **высокий (reflection per-call)** | **высокий (reflection)** | средний (interpret schema) |
| **type-safety** | **🏆 статич.: опечатка поля/тега = COMPILE-ERROR; mismatch = typed `DeError`+path** | статич. = | статич. = | статич. = | **stringly теги — опечатка КОМПИЛИТСЯ, silent-loss** | **аннотации-строки, опечатка молча** | статич.-ish (schema отдельно от типа — drift) |
| **format-agnostic** | **🏆 data-model agnostic (JSON сейчас, TOML/binary позже без перекомпиляции типа) + единый `#impl`** | agnostic (эталон) = | agnostic, но conformance+ручной enum | agnostic (pluggable) = | **JSON-only** | формат-плагины (но reflection) | **JSON-only** |
| **record auto-derive** | **🏆 `#impl` memberwise (flat/nested/generic/recursive) СЕЙЧАС** | `#[derive]` = | synth (struct ✅) = | plugin = | reflection | reflection | ручной schema |
| **enum/sum auto-derive + tagging** | **🟡 external default + internal/adjacent/untagged — гейт на sum-synth (`[M-126-sum-*-rich]`), 180.2** | все 4 режима (эталон) | **ручной `init(from:)`** для assoc-value enum | `@JsonClassDiscriminator` хорошо = | **нет sum-типов** | `@JsonTypeInfo` (verbose) | union-схемы (ручные) |
| **unknown-field-policy** | **= IGNORE by-default (forward-compat, serde-паритет) + opt-in `deny_unknown_fields`** | ignore default; `deny_unknown_fields` opt-in = | ignore default | configurable | **ignore молча** (footgun) | ignore default | configurable |
| **numeric fidelity** | **= JSON→f64-граница 2^53 явно проверена (`OutOfRange`/`LossyInteger`), не silent** | serde_json `Number` arbitrary-prec (строго лучше) | Codable f64-граница (=) | (=) | `json.Number` string-preserving (лучше) | BigInteger/BigDecimal (лучше) | number-граница JS (=) |
| **recursion-depth** | **= configurable max-depth (default 128) → typed `DeError{DepthLimitExceeded}`, не crash** | serde_json `recursion_limit` 128 = | depth-cap | depth-cap | depth-cap | `maxNestingDepth` | runtime |
| **attributes** | **= `#serde(rename/rename_all/skip/default/flatten/tag/content/untagged/deny_unknown_fields)`** | полный набор (эталон) = | `CodingKeys` (нет flatten) | `@SerialName/@Transient/…` = | теги (узко) | богатые (reflection) = | `.transform/.default/.passthrough` = |
| **zero-copy / borrow** | **🟡 позже (§11): borrow `&str`/`&[]u8`** (сейчас owned) | `#[serde(borrow)]` (эталон) | нет (owned) | нет (owned) | нет (копии) | нет | нет |

**Взять:** serde — **data-model + Serializer/Deserializer-разделение + enum-tagging-matrix + attribute-набор + recursion_limit** (золотой стандарт); Swift Codable — **compiler-synth-дух** (container-абстракция Keyed/Unkeyed/SingleValue, `CodingKeys`); Kotlin — **plugin-synth + format-pluggability**; zod — **path-rich validation-ошибки**. **Избегать:** Go/Java **runtime-рефлексию** + JSON-привязку Go + silent tag-typo + silent-unknown-drop; Swift **ручной `init(from:)`** для enum'ов; zod **schema-отдельно-от-типа**. **Доказательство ≥ best-peer построчно:** 🏆 на **derive-mechanism / runtime-cost / type-safety / format-agnostic / record-auto-derive**; **=** на unknown-policy / attributes / depth / numeric (паритет — numeric честно НЕ лучше serde arbitrary-precision, помечено `=` не 🏆); **🟡** на enum-tagging / zero-copy (таргет, гейтнут). **Headline:** единый `#impl`-механизм даёт serde той же машиной, что `==`/`clone`/`hash` — ни один peer не делает serde настолько же неотличимым от равенства.

## 3. Архитектура

**Принцип (auto-derive-precedent, PURE).** serde в Nova — **format-agnostic протокол + value-логика**, БЕЗ I/O-эффекта (чистый кодек над значениями/байтами; net-триада здесь не нужна — нет сокета, нет release-обязательства; module-conventions явно: «PURE codec/serde need NO effect»). Headline-механизм: **компилятор синтезирует тела `@serialize`/`.deserialize` для `record` (Ф.2 «сейчас») и `sum` (Ф.2-sum, гейт 180.2)** через `#impl(Serialize + Deserialize)` — той же машиной, что уже синтезирует `@equal`/`@clone`/`@hash` ([auto-derive-guide.md](../auto-derive-guide.md), `E_AUTO_DERIVE_*`). Атрибуты `#serde(...)` лишь **кастомизируют** (rename/skip/flatten/default/rename_all/tag) или **opt-out**. Всё fallible → `Result[T, SerError|DeError]` (R1/D325).

> **🔑 Это КОМПИЛЯТОРНАЯ работа.** План описывает контракт + форму эмитируемого кода для имплементора (как `#impl(Clone)` synthesize'ит memberwise-копию). Сам план — НЕ код; Ф.2 — расширение auto-derive-движка (synth-pass в `compiler-codegen/src/codegen/emit_c.rs`, рядом с `@clone`/`@equal`-синтезом) на два новых протокола. **Честная граница:** record-synth переиспользует существующий memberwise-проход (✅ landed для record/tuple); **sum-synth НЕ существует** — `[M-126-sum-equal-rich]`/`-clone-rich`/`-hash-rich` OPEN; Plan 172.1 добавил sum-**equality** на codegen-emit-уровне (ДРУГОЙ механизм — структурный per-type `emit_field_eq`, НЕ синтезированный метод в method_table). Поэтому sum-serde — **новая synth-инфра**, гейт.

### Layering diagram

```
┌──────────────────────────────────────────────────────────────────────────┐
│ App     json.encode(user)            │  json.decode[User](s)              │  ← user value-API (Ф.4)
├──────────────────────────────────────┼─────────────────────────────────────┤
│ JsonSerializer (Value→str/[]u8)      │  JsonDeserializer (str/[]u8→Value)  │  ← Ф.4 backend над encoding/json
├──────────────────────────────────────┴─────────────────────────────────────┤
│ Serialize protocol  ·  Deserialize protocol     (format-agnostic, Ф.1)      │  ← контракт
├──────────────────────────────────────────────────────────────────────────┤
│  AUTO-DERIVE synth (компилятор): #impl(Serialize/Deserialize)               │
│    record → memberwise (Ф.2 «сейчас», reuse @clone-машины)                  │  ← Ф.2
│    sum    → match-arm-per-variant (Ф.2-sum, ГЕЙТ [M-126-sum-*-rich]/180.2)  │
│  + container special-cases (Option/[]T/HashMap) — compiler-side (Ф.2)       │  ← НЕ blanket .nv ([M-161])
│  + атрибуты #serde (Ф.3a parser+AST → Ф.3b synth-consume)                   │  ← Ф.3
├──────────────────────────────────────────────────────────────────────────┤
│ Serializer / Deserializer protocols  ·  data-model (bool/int/.../enum/unit) │  ← Ф.1 ядро
├──────────────────────────────────────────────────────────────────────────┤
│  SerError / DeError (структурный OPEN kind + path/location + depth-guard)   │
└──────────────────────────────────────────────────────────────────────────┘
```

**Format-agnostic:** `Serialize`/`Deserialize` НЕ знают про JSON. Тип сериализуется в абстрактную **data-model**, а конкретный **`Serializer`** (json/toml/binary) решает, как это лечь в байты.

### 3.0. Закрытые решения (Q1–Q21 — РЕШЕНЫ)

| # | Вопрос | РЕШЕНИЕ | Обоснование (peer) |
|---|---|---|---|
| Q1 | auto-derive **by-default** vs `#impl`-opt-in | **`#impl(Serialize + Deserialize)` opt-in — ТА ЖЕ аннотация, что `#impl(Equal+Hash+Clone)`.** НЕ «магический default-on». `#serde(...)` кастомизирует/opt-out'ит. | **Консистентность с семейством auto-derive** (`==`/`clone`/`hash` тоже `#impl`-opt-in, [auto-derive-guide.md:6-8](../auto-derive-guide.md)). Default-on сломал бы единообразие и навязал бы serde release-resource-типам. Differentiator = «единый механизм + нулевое тело», НЕ «нулевая аннотация». Rust требует ОТДЕЛЬНЫЙ `#[derive]`-плагин — у Nova одна `#impl`-семья. |
| Q2 | data-model | **Lean serde-набор (12 кейсов): `bool/int/uint/float/str/bytes/option/unit/seq/map/struct/enum`.** Nova `int`=i64, `uint`=u64, `float`=f64, `char`→str-1, кортежи→`seq`, newtype→прозрачен. | serde-модель — золотой стандарт, но 29 методов избыточны (один integer/float-тип). 12 visit-методов vs serde 29. |
| Q3 | shape протокола: visitor (serde) vs leaner | **Serialize = push (generic-bound, см. Q12); Deserialize = pull через `Deserializer` + `Visitor`-callbacks ТОЛЬКО для map/seq/enum.** Скаляры — прямые `deser_X() -> Result[X, DeError]`. | serde Visitor мощно, но 3 уровня тяжелы. Nova: скаляры прямые (Swift `SingleValueContainer`), композиты — driven-visitor. |
| Q4 | enum-tagging default | **Externally-tagged (`{"VariantName": payload}`) — default.** `#serde(tag="t")` internal, `#serde(tag,content)` adjacent, `#serde(untagged)` untagged. Unit-вариант → bare-строка `"VariantName"`. **ГЕЙТ Ф.2-sum/180.2.** | serde-канон. External robust по-умолчанию. |
| Q5 | unknown-field policy: deny vs ignore | **IGNORE by-default (forward-compat), `#serde(deny_unknown_fields)` → opt-in strict (`DeError{UnknownField}`).** | serde default = ignore (API-evolution). Лучше Go (молча ignore, нет opt-in deny). |
| Q6 | это компиляторная работа? | **ДА — Ф.2/Ф.3b/Ф.2-sum = расширение auto-derive synth-движка.** Ф.1 (протоколы/модель/ошибки) + Ф.3a (parser/AST) + Ф.4 (JSON-backend) — `.nv`+parser. | Симметрия с `Clone`/`Equal` synth. Честный gate: typed `json.decode[T]` (record) зависит от Ф.2-record synth. |
| Q7 | missing/optional поля | **`Option[T]`-поле отсутствует → `None`; `#serde(default)` → `T.default()`/zero; иначе missing required → `DeError{MissingField}`.** | serde-семантика. `Option`=genuine absence (R4/D325). Бьёт Go (zero-value молча). |
| Q8 | rename_all-конвенции | **`snake_case`(default Nova)/`camelCase`/`PascalCase`/`SCREAMING_SNAKE_CASE`/`kebab-case`** на типе; поле-override `#serde(rename="id")` сильнее. | serde rename_all (полный набор). Покрывает camelCase JS-API. |
| Q9 | bytes-репрезентация | **`[]u8` → data-model `bytes` (отдельный кейс, НЕ seq-of-int).** JSON-backend → base64-строка (`std/encoding/base64`); binary — нативно. | serde разделяет `serialize_bytes`. JSON не имеет байт-типа. Бьёт `[1,2,3]`-массив (×3). |
| Q10 | SerError vs DeError | **ДВА типа: `SerError` (узкий) и `DeError` (богатый — path/location/kind/source).** Оба OPEN-kind, R5/D325. | Asymmetry: десериализация — главный источник ошибок (нужен path). serde тоже разделяет. |
| Q11 | over `std/encoding/json` | **Backend строится НАД `JsonValue`/`Json.parse`** — НЕ переписываем парсер. `json.decode` = `Json.parse`→`JsonValue`→drive `Deserialize`; `json.encode` = drive `Serialize`→`JsonValue`→`@into()`. | Reuse production-парсера (RFC 8259, surrogate, strict-dup). serde-json тоже слоится над `Value`. |
| Q12 | синтаксис протокол-сигнатур: `impl Trait` param vs generic-bound | **ТОЛЬКО generic-bound скобки `[T Serialize]` / `fn[S Serializer] T @serialize(s mut S)`.** НИКАКОГО `impl Trait` в параметрах. | **`impl Trait`-параметр в `std/*.nv` встречается 0 раз** (verified); канон — `[T Hash]`/`[K Hash + Equal]` (protocols.nv). `impl Trait`-арг — неподтверждённая фича; единый нотейшн обязателен. |
| Q13 | generic-conformance контейнеров (`Option[T]`/`[]T`/`HashMap`) | **Compiler-side monomorphic special-cases в синтезаторе, НЕ blanket-`.nv`-impl.** Синтезатор эмитит `@serialize`/`.deserialize` для container-типа per element-mono. | **`[M-161-parametric-return]` OPEN** (blanket-dispatch V1 = только конкретный return; `fn[T Serialize] Option[T]` рекурсия в element — НЕ покрыта). Зеркало того, как `emit_field_eq` делает container-eq mono (Plan 172.1 `f56cd7b7`/`bd56022e`) — НЕ blanket `.nv`. |
| Q14 | recursion-depth (stack-DoS) | **Configurable `max_depth` в `JsonDeserializer` (default 128) → `DeError{DepthLimitExceeded, path}`. Применяется к ОБЕИМ сторонам (deser-driver И serialize-recursion).** | serde_json `recursion_limit` 128; Go/Jackson depth-cap. Фиберы — bounded stack → overflow = crash/DoS. §8.0 запрещает forward этого на критическом пути. |
| Q15 | numeric-fidelity int>2^53 | **`JsonDeserializer` для `int`/`uint`-target проверяет «f64 — точное целое в безопасном диапазоне»: не-целое ИЛИ \|v\|≥2^53 → `DeError{LossyInteger}`; вне i64/u64 → `OutOfRange`; negative→uint → `OutOfRange`.** Проверка ДО coercion. | `JsonValue.Num`=f64 (json.nv:96) теряет точность >2^53 SILENTLY. serde_json буферит arbitrary-prec; Nova честно ОТКЛОНЯЕТ lossy вместо silent. binary-backend позже — без потерь. Документируем как known JSON-граница. |
| Q16 | map-ключи не-`str` | **v1: только `HashMap[str, V]`. Прочие `K` → compile-error `E_SERDE_NONSTRING_MAP_KEY`** (поле названо). | JSON-ключи всегда строки. Go-style (только string/TextMarshaler). AI-friendly: compile-time, не runtime-silent. Key-codec — §11. |
| Q17 | untagged/internal-tag буферизация | **untagged + internal/adjacent ТРЕБУЮТ self-describing (буферизующего) `Deserializer`; для JSON это бесплатно (`JsonValue` = буфер). Для non-self-describing форматов — НЕ поддержано (serde-правило).** | serde: untagged needs `deserialize_any` + `Content`-буфер. JSON DOM-материализован → retry-variant бесплатен. **Plan 178 typed `.json[T]` нужен ТОЛЬКО external-default → гейт 182 НЕ зависит от буфер-машины.** |
| Q18 | duplicate-field policy | **`Json.parse` уже отклоняет dup-ключи СТРОГО (`DuplicateKey`, json.nv:614-618) ДО serde.** serde-слой `DeError{DuplicateField}` срабатывает только для **flatten-коллизий** (один логический ключ из двух источников). | Консистентно с json.nv strict-`DuplicateKey` («duplicate почти всегда ошибка», AI-friendly). Бьёт Go (last-wins silent). |
| Q19 | float non-finite (NaN/Inf) | **JSON-backend: NaN/Inf → `SerError{NonFiniteFloat}`** (RFC 8259 не допускает); binary — нативно. Deser: `JsonValue.Num` всегда finite. | serde-json default. Бьёт молчаливый `null`/`"NaN"` JS. |
| Q20 | attr-namespace `#serde` vs `#json` | **ЕДИНЫЙ `#serde(...)`; format-scoped/направление-ключи внутри (Q21), нет конкурирующего `#json`.** | Один attribute-словарь — нет drift. serde тоже один `#[serde]` для всех форматов. |
| Q21 | per-format / направление имён | **Конвенции per-format → на BACKEND** (`json.encode_with(v, .{rename_all: CamelCase})` / `decode_with`); тип хранит каноничные имена, поля БЕЗ атрибутов. **Направления** (ser-имя ≠ de-имя; приём алиасов) → `#serde(rename(ser="x", de="y"))` + `#serde(alias=["a","b"])`. **Произвольно РАЗНЫЕ имена per-format** (не конвенция) → **НЕ в auto-derive** (ломает narrow-waist) → wrapper-тип на формат ИЛИ ручной `#impl`-opt-out. | serde: `rename(serialize/deserialize)` есть; per-format-произвол serde тоже толкает в отдельные типы (узкая талия = одно каноническое описание). Backend-конвенция (camelCase JSON vs snake_case TOML) покрывает ~90% без атрибутов на полях. |

### 3.1. Data-model — format-agnostic ядро

12 кейсов (Q2). Это **абстрактный словарь**, на котором говорят `Serialize` и `Serializer`; конкретный формат материализует его в байты. Имя `DataModel` — для документации; протоколы НЕ строят промежуточное DataModel-дерево (push прямо в `Serializer`), enum-перечисление фиксирует контракт visit-методов.

```nova
module encoding.serde

/// Format-agnostic data-model (lean serde, 12 кейсов). Serialize-сторона
/// ОПИСЫВАЕТ значение в этих терминах; Serializer-бэкенд РЕШАЕТ репрезентацию.
/// Nova int=i64/uint=u64/float=f64, char→str-1, кортежи→seq, newtype прозрачен.
#unstable(since = "0.1")
export type DataModel
    | Bool   | Int   | Uint  | Float        // скаляры
    | Str    | Bytes                        // строка / сырые байты (JSON: bytes→base64, Q9)
    | Option                                // Some(inner) | None
    | Unit                                  // () / unit-вариант
    | Seq    | Map                          // []T / HashMap[str,V]
    | Struct | Enum                          // record / sum (auto-derive Ф.2)
```

### 3.2. `Serializer` — push-протокол (бэкенд-точка расширения)

`Serialize`-сторона **толкает** структуру значения в `Serializer`. Generic-bound нотейшн (Q12). Составные возвращают **must-consume под-сериализатор** (гарантия `end()` через consume D133).

```nova
/// Бэкенд сериализации. Реализуют JsonSerializer (Ф.4), TomlSerializer (позже).
/// Каждый метод fallible (формат-write может упасть: non-finite float Q19).
#unstable(since = "0.1")
export type Serializer protocol {
    // ── скаляры ──
    mut @serialize_bool(v bool)   -> Result[(), SerError]
    mut @serialize_int(v int)     -> Result[(), SerError]
    mut @serialize_uint(v u64)    -> Result[(), SerError]
    mut @serialize_float(v f64)   -> Result[(), SerError]   // JSON: NaN/Inf → SerError (Q19)
    mut @serialize_str(v str)     -> Result[(), SerError]
    mut @serialize_bytes(v []u8)  -> Result[(), SerError]   // JSON: base64-str (Q9)
    mut @serialize_unit()         -> Result[(), SerError]   // JSON: null
    mut @serialize_none()         -> Result[(), SerError]   // Option::None  (JSON: null)
    mut @serialize_some[T Serialize](v T) -> Result[(), SerError]
    // ── составные: возвращают must-consume под-сериализатор ──
    mut @serialize_seq(len Option[int])       -> Result[SeqSerializer, SerError]
    mut @serialize_map(len Option[int])       -> Result[MapSerializer, SerError]
    mut @serialize_struct(name str, len int)  -> Result[StructSerializer, SerError]
    // ── enum (tagging Ф.5/180.2; default external) ──
    mut @serialize_variant(enum_name str, idx int, variant str, payload VariantPayload)
        -> Result[(), SerError]
}

export type SeqSerializer consume protocol {           // must-consume → гарантия end()
    mut @element[T Serialize](v T) -> Result[(), SerError]
    consume @end()                 -> Result[(), SerError]
}
export type MapSerializer consume protocol {
    mut @entry[V Serialize](k str, v V) -> Result[(), SerError]   // ключ только str (Q16)
    consume @end() -> Result[(), SerError]
}
export type StructSerializer consume protocol {
    mut @field[T Serialize](key str, v T) -> Result[(), SerError]   // key уже rename-resolved
    mut @skip_field(key str)               -> Result[(), SerError]  // #serde(skip_serializing_if_none)
    consume @end() -> Result[(), SerError]
}
export type VariantPayload | UnitV | NewtypeV(JsonValue) | StructV([]FieldRepr) | SeqV([]JsonValue)
```

`SeqSerializer`/`MapSerializer`/`StructSerializer` — **concrete associated types** конкретного backend'а (Q12: НЕ экзистенциалы). JSON-backend объявляет их как concrete-типы поверх `JsonValue`-стека.

### 3.3. `Deserialize` / `Deserializer` — driven-pull + Visitor для композитов

`Deserializer` держит руль. Скаляры — прямые `deser_X()`. Композиты — `Visitor`-callbacks (`SeqAccess`/`MapAccess`). Generic-bound нотейшн (Q12).

```nova
/// Бэкенд десериализации. Реализуют JsonDeserializer (Ф.4) и др.
#unstable(since = "0.1")
export type Deserializer protocol {
    // ── скаляры (прямой pull) ──
    mut @deser_bool()  -> Result[bool, DeError]
    mut @deser_int()   -> Result[int, DeError]      // Q15: точное-целое-check
    mut @deser_uint()  -> Result[u64, DeError]      // Q15: negative/range-check
    mut @deser_float() -> Result[f64, DeError]
    mut @deser_str()   -> Result[str, DeError]
    mut @deser_bytes() -> Result[[]u8, DeError]     // JSON: base64-decode (Q9)
    mut @deser_unit()  -> Result[(), DeError]
    mut @deser_option[V Visitor](v mut V) -> Result[(), DeError]   // null→visit_none / иначе visit_some
    // ── составные (driven, format вызывает visitor; depth-guard Q14) ──
    mut @deser_seq[V Visitor](v mut V)    -> Result[(), DeError]
    mut @deser_map[V Visitor](v mut V)    -> Result[(), DeError]
    mut @deser_struct[V Visitor](name str, fields []str, v mut V) -> Result[(), DeError]
    mut @deser_enum[V Visitor](name str, variants []str, v mut V) -> Result[(), DeError]
    mut @deser_any[V Visitor](v mut V)    -> Result[(), DeError]   // self-describing (untagged Q17)
}

/// Driven-callbacks. Auto-derive синтезирует Visitor-impl per тип (Ф.2).
#unstable(since = "0.1")
export type Visitor protocol {
    @visit_seq[A SeqAccess](access mut A) -> Result[Self, DeError] => Err(DeError.unexpected("seq"))
    @visit_map[A MapAccess](access mut A) -> Result[Self, DeError] => Err(DeError.unexpected("map"))
    @visit_none()                         -> Result[Self, DeError] => Err(DeError.unexpected("null"))
    @visit_some[D Deserializer](d mut D)  -> Result[Self, DeError]
    // дефолтные арм-ы → DeError → тип переопределяет нужные
}
export type SeqAccess protocol { mut @next[T Deserialize]() -> Result[Option[T], DeError] }
export type MapAccess protocol {
    mut @next_key()                       -> Result[Option[str], DeError]
    mut @next_value[V Deserialize]()      -> Result[V, DeError]
}

/// Конструкция значения из любого формата. `deserialize` — статический (D35).
#unstable(since = "0.1")
export type Deserialize protocol {
    .deserialize[D Deserializer](d mut D) -> Result[Self, DeError]
}
/// Зеркальный push-протокол.
#unstable(since = "0.1")
export type Serialize protocol {
    @serialize[S Serializer](s mut S) -> Result[(), SerError]
}
```

**Скаляры/контейнеры — conformant (Q13, compiler-side, НЕ blanket `.nv`).** `bool/int/u64/f64/str/[]u8` получают `Serialize`/`Deserialize`-impl **синтезом в компиляторе** (как built-in routines в `@equal`-synth). `Option[T]`, `[]T` (→seq), `HashMap[str,V]` (→map) — синтезатор эмитит **monomorphic per-element** impl (зеркало `register_container_eq_mono`, Plan 172.1), потому что blanket-`.nv` `[T Serialize] Option[T]` упирается в OPEN `[M-161-parametric-return]`. **Ф.0 верифицирует**, можно ли часть выразить как `.nv` после возможного landing 161-parametric — иначе всё compiler-side.

### 3.4. AUTO-DERIVE — компиляторный synth-контракт (Ф.2 record; Ф.2-sum гейт)

**Главное.** Компилятор synthesize'ит тела `@serialize`/`.deserialize` для типа с `#impl(Serialize + Deserialize)` **структурно**, ровно как `#impl(Clone)` даёт memberwise `@clone` ([auto-derive-guide.md](../auto-derive-guide.md)). **Триггер — `#impl`-аннотация (Q1)**, как у всего семейства auto-derive (НЕ default-on).

**Контракт synth (имплементору) — эмитируемая форма для `#impl(Serialize+Deserialize) type User { id int, name str, email Option[str] }`:**

```nova
// СИНТЕЗИРУЕТСЯ компилятором (НЕ пишется руками) — форма emitted-кода:
fn[S Serializer] User @serialize(s mut S) -> Result[(), SerError] {
    mut st = s.serialize_struct("User", 3)?          // len = число НЕ-skip полей
    st.field("id", @id)?                              // key = rename-resolved (Ф.3); value = field
    st.field("name", @name)?
    st.field("email", @email)?                        // Option → serialize_none/some рекурсивно
    st.end()
}
fn[D Deserializer] User.deserialize(d mut D) -> Result[User, DeError] {
    d.deser_struct("User", ["id", "name", "email"], mut UserVisitor.new())?   // Visitor synth тоже
}
// UserVisitor.visit_map: цикл next_key → match по rename-resolved ключу →
//   next_value[FieldType] в локал; missing required → DeError{MissingField,path};
//   Option-поле отсутств. → None (Q7); #serde(default) → T.default() (Q7);
//   unknown ключ → ignore (Q5 default) | DeError{UnknownField} (#serde(deny_unknown_fields)).
```

**RECORD (Ф.2 «сейчас») — переиспользование машины:** тот же memberwise-проход, что `Clone`/`Equal` (обход полей record, рекурсия в field-type). Field-eligibility: каждое поле `Serialize`/`Deserialize` (примитив, container-mono Q13, или само `#impl`-derived) — иначе `E_SERDE_FIELD_NOT_SERIALIZABLE` (зеркало `E_AUTO_DERIVE_FIELD_LACKS_PROTOCOL`). `priv`-поля сериализуются (structural-привилегия synth, как `@clone`/`@equal` видят все поля). User-override (`fn T @serialize`) wins (D77).

**SUM (Ф.2-sum — 🔴 ГЕЙТ `[M-126-sum-*-rich]`/180.2, НЕ «сейчас»):** synth для `sum` требует match-arm-per-variant эмиссии (`| V1(..) | V2{..} | V3` → `serialize_variant(idx, name, payload)`; deser — `deser_enum` + tag-диспетч). **Эта synth-инфра на main НЕ существует** — `[M-126-sum-equal-rich]`/`-clone-rich`/`-hash-rich` OPEN; Plan 172.1 дал sum-**equality** на codegen-emit-уровне (ДРУГОЙ механизм). **Честный план:** Ф.2-sum **гейтнут** на закрытии sum-rich-auto-derive-семейства (или выделяется в **под-план 180.2** с собственной synth-работой). Поскольку Plan 178 typed `.json[User]` нужен только record-DTO — **гейт 182 открывается record-only** (§4 Ф.4).

**Cycle-detection (compile-time):** прямой `A`-в-`A` через value-embed → `E_SERDE_DERIVE_CYCLE` (зеркало `E_AUTO_DERIVE_CYCLE`); рекурсия через `[]A`/`*A`/`Option[A]` (heap-indirect) — ОК структурно. **Runtime-cycle (heap-indirect граф с обратной ссылкой)** — §3.4a depth-guard ловит (typed `DeError`/`SerError{DepthLimitExceeded}`, НЕ crash); полноценная cycle-detection (`@JsonIdentityInfo`-аналог) — scope-out §11 (serde тоже overflow'ит на циклах, документируем).

### 3.4a. Depth-guard (Q14) — обе стороны

Десериализация и сериализация рекурсивны → adversarial-вложенность = stack-overflow на bounded-fiber-стеке. **Решение:** `JsonDeserializer`/`JsonSerializer` несут счётчик глубины (`max_depth` default 128, конфигурируемо). Каждый `deser_struct`/`deser_seq`/`deser_map` (и serialize-аналоги) инкрементит при входе, декрементит при выходе; превышение → `DeError{DepthLimitExceeded, path}` / `SerError{DepthLimitExceeded, path}` ДО рекурсивного спуска. **Json.parse-сторона:** Ф.0 аудитит, есть ли в `Json.parse` собственный depth-bound (рекурсивный спуск парсера ТОЖЕ overflow'ит); если нет — флагается как `std/encoding/json` prereq в §9 (добавить depth-guard в парсер ИЛИ обернуть `Json.parse` в bounded-driver).

### 3.5. Атрибуты `#serde(...)` (Ф.3) — кастомизация synth

**🟡 Attribute-инфра НЕ существует** (Ф.3 расщеплён): AST `RecordField`/`SumVariant` **не имеют `attrs`-поля**; парсер знает hardcoded type-attrs (`#stable`/`#deprecated`/…) + **field-level `#visible_to`** (parser/mod.rs:4315 — единственный field-attr-прецедент). Поэтому:

- **Ф.3a (parser + AST + validation):** добавить `attrs: Vec<SerdeAttr>` в `RecordField`, `SumVariant`, и type-level `TypeAttr`-расширение; распарсить общий `#serde(key)` / `#serde(key="val")` / `#serde(key=ident)` (расширяя `#visible_to`-механику на key=value); статическая валидация `E_SERDE_BAD_ATTRIBUTE` (опечатка ключа/невалидное значение/несовместимая комбинация). **§3.0-решение:** инфра **serde-specific** в v1 (не общий reusable `#name(k=v)` — это отдельный язык-feature; serde-specific быстрее и безопаснее).
- **Ф.3b (synth-consumption):** synth Ф.2 читает `attrs` и **меняет эмитируемое тело** (rename-resolved ключи, skip-арм, flatten-инлайн, default-фолбэк).

Алиаса `#json` НЕТ (Q20 — единый `#serde`).

| Атрибут | Позиция | Эффект на synth |
|---|---|---|
| `#serde(rename="id")` | поле/вариант | wire-ключ = `"id"`; сильнее `rename_all` (Q8) |
| `#serde(rename_all="camelCase")` | тип | все поля/варианты → конвенция (5 кейсов, Q8) |
| `#serde(rename(ser="x", de="y"))` | поле/вариант | разные wire-имена на запись/чтение (направления, Q21) |
| `#serde(alias=["x","y"])` | поле | deser ДОПОЛНИТЕЛЬНО принимает эти имена (ser не меняется, Q21) |
| `#serde(skip)` | поле | НЕ сериализуется И НЕ десериализуется (требует `default` для deser, иначе `E_SERDE_SKIP_WITHOUT_DEFAULT`) |
| `#serde(skip_serializing_if_none)` | поле `Option[T]` | `None` → `st.skip_field` (опускает ключ); deser принимает отсутствие |
| `#serde(default)` | поле | missing при deser → `T.default()` (не `MissingField`, Q7) |
| `#serde(flatten)` | поле-record | поля вложенного record инлайнятся в родителя (один уровень) |
| `#serde(deny_unknown_fields)` | тип | unknown-ключ → `DeError{UnknownField,path}` (иначе ignore, Q5) |
| `#serde(tag="type")` | sum-тип | internally-tagged (Ф.5/180.2, Q4) |
| `#serde(tag="t", content="c")` | sum-тип | adjacently-tagged (Ф.5/180.2) |
| `#serde(untagged)` | sum-тип | untagged — try-each-variant через буфер (Q17, Ф.5/180.2) |

`flatten` + `deny_unknown_fields` несовместимы → `E_SERDE_FLATTEN_DENY_CONFLICT`. Невалидный атрибут → `E_SERDE_BAD_ATTRIBUTE` (compile-time — бьёт Go silent tag-typo).

**Per-format имена (Q21).** Атрибуты выше задают **каноничное** имя/конвенцию для ВСЕХ форматов (narrow-waist). Расхождение между форматами — НЕ per-field-per-format атрибут, а:
- **конвенция формата** (camelCase JSON vs snake_case TOML) → **опция backend**: `json.encode_with(v, .{rename_all: CamelCase})` / `json.decode_with(s, .{...})`; тип не трогается;
- **направление** (имя на запись ≠ чтение, приём алиасов) → `#serde(rename(ser=…, de=…))` / `#serde(alias=[…])`;
- **произвольно разные имена per-format** (не конвенция) → **wrapper-тип на формат или ручной `#impl`-opt-out** (как serde — узкая талия держит ОДНО каноническое описание; авто-вывод этим не нагружаем).

### 3.6. Enum-tagging (Ф.5 — 🟡 ГЕЙТ Ф.2-sum/180.2)

Для `type Shape | Circle{r f64} | Square{side f64}` (Q4). **Весь блок гейтнут на sum-synth-инфре** (Ф.2-sum):

| Стратегия | Атрибут | Wire (Circle) |
|---|---|---|
| **External** (default) | — | `{"Circle":{"r":1.0}}` |
| Internal | `#serde(tag="kind")` | `{"kind":"Circle","r":1.0}` |
| Adjacent | `#serde(tag="t",content="c")` | `{"t":"Circle","c":{"r":1.0}}` |
| Untagged | `#serde(untagged)` | `{"r":1.0}` (try-each-variant) |

Unit-вариант (`| Pending`) → external/internal: bare-строка `"Pending"`. Deser-диспетч: external — один-ключ-объект → имя варианта; internal — читает `tag`-поле; adjacent — `tag`+`content`; untagged — **буфер `JsonValue`** (Q17, для JSON бесплатно) → пробует варианты по порядку, первый успешный (`DeError{NoVariantMatched}` если все failed). Internal-tagging запрещён на варианте-с-непоименованным-payload (newtype над не-struct) → `E_SERDE_INTERNAL_TAG_NON_STRUCT` (serde-семантика). **untagged-footgun** (неоднозначные варианты → first-match) — документируется.

### 3.7. `SerError` / `DeError` — структурные OPEN-kind (R5/D325)

Asymmetry (Q10): сериализация почти не падает → узкий `SerError`; десериализация — главный источник ошибок → богатый `DeError` с **path** + **location** + **source-chain**.

```nova
/// Ошибка сериализации (узкая). R5/D325 — один структурный тип, OPEN kind.
#unstable(since = "0.1")
export type SerError value { ro kind SerErrorKind, ro path str }
export type SerErrorKind
    | NonFiniteFloat                 // NaN/Inf в JSON (Q19)
    | DepthLimitExceeded             // serialize-сторона depth-guard (Q14/§3.4a)
    | Custom(str)
    | Other(str)                     // OPEN → wildcard-arm обязателен
export fn SerError @to_str(self) -> str

/// Ошибка десериализации (богатая: path + location → хорошие сообщения).
#unstable(since = "0.1")
export type DeError value {
    ro kind     DeErrorKind
    ro path     str                  // JSONPath-стиль: "$.users[2].email"
    ro location Option[Location]     // line/col из Json.parse (если есть)
    ro source   Option[*DeSource]    // source-chaining ParseJsonError / Utf8Error / Base64Error
}
export type DeErrorKind
    | UnexpectedType { expected str, found str }   // ждали int, нашли string
    | MissingField(str)                            // required-поле отсутствует (Q7)
    | UnknownField(str)                            // при deny_unknown_fields (Q5)
    | UnknownVariant { name str, expected []str }  // enum tag не совпал
    | NoVariantMatched                             // untagged — ни один вариант (Q17)
    | InvalidLength { expected int, found int }    // tuple/fixed-seq
    | DuplicateField(str)                          // flatten-коллизия (Q18; parse-level dup ловит json.nv)
    | OutOfRange(str)                              // int не влез в i64/u64 (Q15)
    | LossyInteger(str)                            // f64 не точное целое / |v|≥2^53 (Q15)
    | DepthLimitExceeded                           // deser-сторона depth-guard (Q14)
    | NonStringMapKey                              // (зарезервировано; реально compile-time Q16)
    | Syntax                                       // обёртка ParseJsonError (Q11)
    | Custom(str)
    | Other(str)                                   // OPEN → wildcard обязателен
export type Location value { ro line int, ro col int }
priv type DeSource | Json(ParseJsonError) | Utf8(Utf8Error) | Base64(Base64Error)
export fn DeError @to_str(self) -> str             // "at $.users[2].email (line 5, col 12): expected int, found string"
```

`DeError.path` строится **driven**: каждый `deser_struct`-field-арм / `deser_seq`-index добавляет сегмент при пробросе (`?`-rethread с path-push). Это даёт serde+zod-class диагностику, недоступную Go-reflection.

### 3.8. JSON-backend (Ф.4) — над `std/encoding/json`, UNBLOCKS Plan 178

`JsonSerializer`/`JsonDeserializer` слоятся над существующим `JsonValue`/`Json.parse` (Q11) — НЕ переписывают парсер. Это **точка, которую потребляет Plan 178** для **record-DTO** ([178:9/217/428](178-std-http.md)).

```nova
// std/encoding/serde/json.nv
import std.encoding.json.{JsonValue, Json, ParseJsonError}

/// Сериализатор: drive Serialize → строит JsonValue → @into() компактный JSON.
/// Несёт depth-counter (Q14). SeqSerializer/MapSerializer/StructSerializer =
/// concrete associated types над JsonValue-стеком (Q12).
export type JsonSerializer value { /* priv: стек JsonValue-узлов, depth int, max_depth int */ }
/// Десериализатор: Json.parse → JsonValue → drive Deserialize. depth-counter (Q14).
export type JsonDeserializer value { /* priv: курсор по JsonValue-дереву, depth int, max_depth int */ }

// ── ПУБЛИЧНЫЙ API (то, что зовёт Plan 178 и пользователь) ──
/// T → компактный JSON. #impl(Serialize)-derived (Ф.2). Result (R1/D325).
#unstable(since = "0.1")
export fn json.encode[T Serialize](v T) -> Result[str, SerError]
/// T → pretty JSON (2-space).
export fn json.encode_pretty[T Serialize](v T) -> Result[str, SerError]
/// T → JsonValue (DOM, без строкования).
export fn json.to_value[T Serialize](v T) -> Result[JsonValue, SerError]
/// JSON-строка → T. Json.parse + drive Deserialize. Result с path/location.
#unstable(since = "0.1")
export fn json.decode[T Deserialize](s str) -> Result[T, DeError]
/// []u8 (UTF-8) → T (байт-first вход; Plan 178 Body.@json).
export fn json.decode_bytes[T Deserialize](b []u8) -> Result[T, DeError]
/// JsonValue (уже распарсенный DOM) → T.
export fn json.from_value[T Deserialize](v JsonValue) -> Result[T, DeError]
/// Конфигурируемый max-depth (Q14); default 128 в decode/decode_bytes.
export fn json.decode_with[T Deserialize](s str, max_depth int) -> Result[T, DeError]
```

**Контракт для Plan 178:** `Body.@json[T](self) -> Result[T, HttpError]` ([178:217](178-std-http.md)) = `json.decode_bytes[T](body_bytes)`, маппя `DeError → HttpError{Protocol(de.to_str())}` (source-chaining). `RequestBuilder.@json[T](v T)` ([178:428](178-std-http.md)) = `json.encode[T](v)` + `Content-Type: application/json`. **Dynamic `.json() -> JsonValue`** Plan 178 уже имеет напрямую через `encoding/json`; **typed `.json[T]` для record-DTO гейтится Ф.2-record + Ф.4** — это разблокировка [178:9](178-std-http.md). **sum-DTO** в HTTP-payload (редко) → ждёт Ф.2-sum/180.2 (честный sub-gate).

**Numeric-фиделити (Q15):** `JsonValue.Num`=f64 (json.nv:96). `json.decode[T]` в `int`/`uint`-поле проверяет «f64 — точное целое в `[-2^53, 2^53]`»: не-целое ИЛИ `|v|≥2^53` → `DeError{LossyInteger}`; вне i64/u64 → `OutOfRange`; negative→uint → `OutOfRange`. **Никакого silent-lossy.** u64>2^53 — известная JSON-граница (документируется; binary-backend позже без потерь). `bytes`-поле ↔ base64 (Q9, `std/encoding/base64`). **`Json.parse` raw-token:** Ф.0 верифицирует, выдаёт ли `Json.parse` сырой числовой токен ДО f64-coercion (для точного >2^53); если нет — `LossyInteger`-проверка работает на f64 (отклоняет lossy честно), а arbitrary-precision — §11.

## 4. Фазы

**Dep-chain:** Ф.0 → Ф.1 → **Ф.2-record (compiler auto-derive — core)** → Ф.3a → Ф.3b → Ф.4 → [Ф.2-sum + Ф.5 — гейтнуты]. **«сейчас»:** Ф.0–Ф.4 (record-only; Ф.4 = JSON-backend, UNBLOCKS Plan 178 typed `.json[T]` для record-DTO). **«позже» (гейт sum-synth/180.2):** Ф.2-sum (sum-auto-derive), Ф.5 (enum-tagging-strategies). **«потом» (вне плана, §11):** TOML/YAML/binary backends, zero-copy/borrow, schema-gen, versioning, runtime-cycle-detection, key-codec для не-str-ключей, arbitrary-precision numbers. Коммит после каждой фазы, no-amend (§10).

- **Ф.0 — GATE (без кода). «сейчас».** Закрыть §3.0 (Q1–Q21); написать **D340–D346 spec-first** (§5); **verify/renumber D-номеров** (high-water=D332 Plan 178 → serde старт **D340**; ⚠ если 177/178/179 ещё не в `spec/decisions/` — зафиксировать gap-ноту как Plan 177 §«зарезервированы», взять D340+ безусловно). **Verify на main (КРИТИЧНО — закрывает critic-gaps):** (1) **record-auto-derive-машина** `#impl(Equal/Clone/Hash)` — где живёт memberwise-synth (`emit_c.rs`), это **тот же seam**, в который Ф.2-record встраивает `Serialize`/`Deserialize`; (2) **🔴 sum-auto-derive `[M-126-sum-equal-rich]`/`-clone-rich`/`-hash-rich` — OPEN?** (verified OPEN на main) → зафиксировать Ф.2-sum как ГЕЙТ, НЕ reuse; Plan 172.1 sum-eq = ДРУГОЙ механизм; (3) **🔴 `[M-161-parametric-return]` — OPEN?** (verified OPEN) → решить: container-conformance (`Option[T]`/`[]T`/`HashMap`) = compiler-side mono special-cases (НЕ blanket `.nv`), зеркало `register_container_eq_mono`; verify, выразим ли `fn[T Serialize] Option[T] @serialize` хоть частично как `.nv`; (4) **🟡 attribute-инфра** — verify AST `RecordField`/`SumVariant` без `attrs` (verified), field-attr-прецедент `#visible_to` (parser:4315) → Ф.3a-объём; (5) **`std/encoding/json`** `JsonValue`/`Json.parse`/`@into()`/strict-`DuplicateKey` стабильны; **аудит depth-bound в `Json.parse`** (Q14 — есть ли защита от stack-DoS на парс-стороне; если нет → §9-prereq); **аудит raw-numeric-token** (Q15). **GATE.** DEP: Plan 177 (✅), `std/encoding/json` (✅), auto-derive-машина record (✅).
- **Ф.1 — protocols + data-model + errors. «сейчас».** Чистые типы+протоколы, **БЕЗ I/O-effect** (§9 PURE). (1) **Data-model** `DataModel` (12 кейсов, §3.1) — lean serde, не дублирует `JsonValue`. (2) **Протоколы** `Serialize`/`Deserialize`/`Serializer`/`Deserializer`/`Visitor`/`*Access` — **generic-bound нотейшн `[T Serialize]`** (Q12, НЕ `impl Trait`); concrete associated sub-serializers (§3.2). (3) **Ошибки** `SerError`/`DeError` структурные, OPEN `ErrorKind` + **path/location/source** (§3.7, D325 R1/R5). (4) **Скаляр-conformance** для `bool/int/u64/f64/str/[]u8` — где выразимо как `.nv` (verify Ф.0), иначе compiler-side-флаг для Ф.2. spec: D340. pos: ручной `Serialize`/`Deserialize` impl на toy-record через протоколы; `DeError` несёт непустой `path`. neg: `Fail[E]` для своей ошибки → `EXPECT_COMPILE_ERROR` (D325 R5); `try_` на не-from-методе → reject. DEP: Ф.0, Plan 177.
- **Ф.2 — COMPILER AUTO-DERIVE **RECORD** (структурный синтез). «сейчас». 🎯 ядро differentiator'а (record-часть).** **Работа в КОМПИЛЯТОРЕ** (план описывает контракт+emitted-shape для имплементора — §3.4/§9). Через `#impl(Serialize + Deserialize)` (Q1) синтезирует `Serialize`+`Deserialize` для **record** структурно, переиспользуя memberwise-машину `#impl(Clone)`: (a) record `{f1 T1,..,fN TN}` → `serialize_struct(name, N)` + per-field `.field("fi", @fi)?` (требует `Ti: Serialize` — иначе **E_SERDE_FIELD_NOT_SERIALIZABLE** с полем-нарушителем, НЕ silent-drop); deserialize — `deser_struct` + Visitor-сборка, missing required → `DeError{MissingField,path}`; (b) **container-conformance compiler-side** (Q13): `Vec[T]`/`Option[T]`/`HashMap[str,V]`/nested-record — synth эмитит monomorphic-impl per element (НЕ blanket `.nv`, `[M-161]`-обход); (c) **depth-guard** (Q14) встроен в JSON-backend-driver (Ф.4), synth-сторона рекурсивна но bounded; (d) synth — **opt-out** (`#serde(skip)` на типе — Ф.3) и **НЕ срабатывает** для типов с не-сериализуемым полем (resource/must-consume, raw-pointer, `HashMap[non-str,_]` Q16) → **E_SERDE_FIELD_NOT_SERIALIZABLE**/**E_SERDE_NONSTRING_MAP_KEY** (поле названо, НЕ silent). **🔴 SUM — НЕ в этой фазе** (Ф.2-sum гейт, ниже). spec: D341 (record-auto-derive-контракт), D342 (data-model↔synth mapping). pos: round-trip auto record (flat/nested/generic-`Vec`/`Option`/`HashMap[str,_]`/рекурсивный `Tree{val int, kids []Tree}`); **синтез через `#impl`, нулевое тело**. neg: record с must-consume-полем → `EXPECT_COMPILE_ERROR` E_SERDE_FIELD_NOT_SERIALIZABLE (поле названо); raw-pointer-поле → reject; `HashMap[int,_]` → E_SERDE_NONSTRING_MAP_KEY; **silent field-drop НЕ происходит** (тест: все поля в выводе). DEP: Ф.1, **record-memberwise-seam (✅ landed)**.
- **Ф.3a — attribute parser + AST + validation. «сейчас».** Добавить `attrs: Vec<SerdeAttr>` в `RecordField`/`SumVariant`/type-level (расширяя `#visible_to`-механику parser:4315 на `key`/`key="val"`/`key=ident`); распарсить `#serde(...)`; **статическая валидация** `E_SERDE_BAD_ATTRIBUTE` (опечатка ключа/значения/комбинации). serde-specific инфра (Q20, не общий reusable). spec: D343 (атрибуты+grammar+validation). pos: парс `#serde(rename=…)`/`rename(ser/de)`/`alias=[…]`/`rename_all`/`skip`/`default`/`flatten`/`deny_unknown_fields` на поле/типе → AST-storage (per-format КОНВЕНЦИЯ — опция backend Ф.4, НЕ field-attr; Q21); неизвестный ключ → reject. neg: опечатка ключа → `EXPECT_COMPILE_ERROR` E_SERDE_BAD_ATTRIBUTE; невалидный `rename_all`-литерал → reject. DEP: Ф.0 (объём verified). **Без Ф.3a `json.encode/decode` работает ТОЛЬКО на un-attributed structural-типах.**
- **Ф.3b — synth-consumption атрибутов. «сейчас».** Synth Ф.2 читает `attrs` и меняет тело: `rename`/`rename_all` (rename-resolved ключи); `skip`/`skip_serializing`/`skip_deserializing` (`skip` deser → обязателен `default`, иначе **E_SERDE_SKIP_WITHOUT_DEFAULT**); `default`/`default=expr`; `flatten` (inline + conflict-detect → **E_SERDE_FLATTEN_DENY_CONFLICT** при `deny`); `deny_unknown_fields` (unknown → `DeError{UnknownField}` вместо ignore-default Q5). `tag`/`content`/`untagged` — парсятся в Ф.3a, **потребляются в Ф.5** (sum-гейт). spec: amend D341/D343. pos: `rename`/`rename_all` round-trip; `skip`+`default`; `flatten`-inline; `deny_unknown_fields` ловит лишнее. neg: `skip` deser без `default` → E_SERDE_SKIP_WITHOUT_DEFAULT; `flatten`+`deny` → reject; `deny_unknown_fields`+unknown → runtime `UnknownField` с path. DEP: Ф.2, Ф.3a.
- **Ф.4 — JSON backend над `std/encoding/json`. «сейчас». 🔴 UNBLOCKS Plan 178 (Q20 HARD-GATE, record-DTO).** `JsonSerializer`/`JsonDeserializer` реализуют протоколы Ф.1 поверх **существующего** `JsonValue`/`Json.parse`/`@into()` (Q11 — reuse lexer/escape/round-trip/strict-dup, НЕ дублируем): `json.encode[T]`/`decode[T]`/`encode_pretty[T]`/`to_value[T]`/`from_value[T]`/`decode_bytes[T]`/`decode_with[T]` (§3.8). **depth-guard** (Q14, default 128) в driver. **numeric-fidelity** (Q15): int/uint-target → точное-целое-check → `LossyInteger`/`OutOfRange`. **bytes**↔base64 (Q9). `ParseJsonError` → `DeError{Syntax, source}` (source-chain, line/col не теряются). **Это РОВНО то, что Plan 178 `.json[T]()`/`.json(v T)` консумирует для record-DTO.** spec: D344 (JSON-backend-mapping; path-проброс; numeric; depth; base64). pos: `json.encode`/`decode` round-trip record/nested/`Option`(→absent/null §3.0)/`Vec`/`HashMap[str,_]`; pretty; `to_value`/`from_value`-bridge; e2e `Json.parse(json.encode(v))!! == json.to_value(v)`; numeric-граница (2^53+1 → `LossyInteger`, negative→uint → `OutOfRange`); depth-limit (вложенность >128 → `DepthLimitExceeded`, НЕ crash); bytes↔base64 round-trip. neg: type-mismatch → `DeError` с path (`expected int, found str at $.user.tags[2]`); truncated JSON → `Syntax` (source-chain line/col); `MissingField` с path; `LossyInteger` на 2^53+1 (НЕ silent); deep-nested → `DepthLimitExceeded` (typed, не overflow). DEP: Ф.2, Ф.3b, **`std/encoding/json`** (✅). **🔓 GATE-RELEASE → Plan 178 Ф.json (record-DTO).**
- **Ф.2-sum — COMPILER AUTO-DERIVE **SUM**. 🔴 «позже» — ГЕЙТ `[M-126-sum-*-rich]` / под-план 180.2.** Synth `Serialize`/`Deserialize` для `sum` (match-arm-per-variant: `serialize_variant(idx,name,payload)`; deser — `deser_enum` + tag-диспетч). **Гейт:** sum-rich-auto-derive-инфра (`[M-126-sum-equal-rich]`/`-clone-rich`/`-hash-rich`) **OPEN на main** — Ф.2-sum либо **ждёт их закрытия**, либо выделяется в **180.2** с собственной sum-synth-работой (match-arm emission per variant, payload-кодирование). **НЕ на критическом пути 182** (record-DTO достаточно). spec: D345 (sum-auto-derive + tagging-контракт). pos (когда разгейчено): unary-вариант (`Color | Red | Green` → `"Red"`); payload (`Shape | Circle(f64)` → `{"Circle":1.0}`); struct-вариант (`Event | Click{x int,y int}`); externally-tagged wire проверена. neg: silent variant-drop НЕ происходит. DEP: **`[M-126-sum-*-rich]` (HARD-PREREQ)**, Ф.2, Ф.4.
- **Ф.5 — enum-tagging-strategies. 🟡 «позже» — ГЕЙТ Ф.2-sum.** Конфигурируемый tagging (атрибуты Ф.3a parsed, потребляются тут): `#serde(untagged)` (буфер `JsonValue` Q17 → try-each first-match); `#serde(tag="type")` (internal, только struct-варианты, unit=`{"type":"V1"}`); `#serde(tag,content)` (adjacent). External=default (Ф.2-sum). **Soundness:** `internal` на tuple-варианте → **E_SERDE_INTERNAL_TAG_NON_STRUCT**; untagged-footgun (first-match) документируется; буферизация требует self-describing format (Q17 — для JSON бесплатно). spec: D345 amend. pos: 4 режима round-trip; untagged first-match; internal unit+struct. neg: internal на tuple-варианте → reject; untagged без вариантов → reject; adjacent missing `content` → reject. DEP: Ф.2-sum, Ф.4. **«сейчас» ТОЛЬКО если Plan 178 потребует не-default-tagging — verified НЕ требует (record-DTO + external-default) → deferred.**

---

## 5. Spec / D / Q / docs

**D-номера:** high-water=**D332** (Plan 178 http). serde старт **D340** (gap D333–D339 зарезервированы под Plan 179 compress + резерв; gap-нота как Plan 177 §«зарезервированы», если 177/178/179 ещё не в `spec/decisions/` — verify/renumber в Ф.0).

- **NEW D340** — **serde data-model + протоколы.** Формат-agnostic 12-кейс-модель; `Serialize`/`Deserialize`/`Serializer`/`Deserializer`/`Visitor`/`*Access` — **generic-bound нотейшн `[T Serialize]`** (НЕ `impl Trait`, Q12); concrete associated sub-serializers; `SerError`/`DeError` структурные OPEN-kind + **path/location/source** (D325 R1/R5). PURE (без effect). Рядом с codec-семейством (json/base64).
- **NEW D341** — **record-auto-derive-контракт (компилерный синтез).** `#impl(Serialize+Deserialize)` opt-in (Q1, симметрия с `Equal/Clone/Hash`); memberwise-reuse `@clone`-машины; контракт emitted-shape; `Ti: Serialize` иначе **E_SERDE_FIELD_NOT_SERIALIZABLE** (поле названо, НЕ silent-drop); container-conformance compiler-side mono (Q13, `[M-161]`-обход); cycle-detect compile-time. **Headline-differentiator** (единый `#impl` vs Rust отд.`#[derive]`/Go reflection).
- **NEW D342** — **data-model ↔ синтез mapping.** record→`Struct`, `Vec`→`Seq`, `HashMap[str,_]`→`Map`, `Option`→`Option` (None-политика: absent vs null — §3.0/Ф.4), скаляры→прим, `[]u8`→`Bytes`. Inverse для deser. **numeric-fidelity** (Q15): int/uint точное-целое-check.
- **NEW D343** — **атрибуты `#serde`.** Grammar `#serde(key)`/`(key="v")`/`(key=ident)`; AST `attrs`-расширение `RecordField`/`SumVariant`/type-level (Ф.3a); `rename`/`rename_all`(5)/`skip[_ser/_de]`/`default[=expr]`/`flatten`/`deny_unknown_fields`/`tag`/`content`/`untagged`. **Статическая валидация** (E_SERDE_BAD_ATTRIBUTE / E_SERDE_SKIP_WITHOUT_DEFAULT / E_SERDE_FLATTEN_DENY_CONFLICT / E_SERDE_INTERNAL_TAG_NON_STRUCT). **Unknown-field default=ignore** (Q5), `deny_unknown_fields` opt-in. Единый `#serde` (Q20, нет `#json`).
- **NEW D344** — **JSON-backend над `std/encoding/json`.** `json.encode[T]`/`decode[T]`/`encode_pretty`/`to_value`/`from_value`/`decode_bytes`/`decode_with`; data-model↔`JsonValue`-mapping; **path-проброс DeError**; **depth-guard** (Q14 default 128 → `DepthLimitExceeded`); **numeric-fidelity** (Q15 `LossyInteger`/`OutOfRange`); **bytes**↔base64 (Q9); `ParseJsonError`→`DeError{Syntax,source}` chain; strict-`DuplicateKey` reuse (Q18). **Контракт для Plan 178 `.json[T]` (record-DTO).**
- **NEW D345** — **sum-auto-derive + tagging-strategies (Ф.2-sum/Ф.5).** match-arm-per-variant synth; externally(default)/internally(`tag`)/adjacently(`tag`+`content`)/untagged (буфер Q17). **ГЕЙТ `[M-126-sum-*-rich]`.** Семантика deser, first-match untagged-footgun, struct-only-инвариант internal (E_SERDE_INTERNAL_TAG_NON_STRUCT). amend D341.
- **NEW D346** — **soundness-инварианты: numeric + depth + map-keys.** Q14 depth-guard (обе стороны, default 128); Q15 numeric точное-целое-check (no silent-lossy); Q16 только `HashMap[str,_]` v1 (`E_SERDE_NONSTRING_MAP_KEY`); Q18 dup-field reconcile с json.nv strict-`DuplicateKey`. **Почему отдельный D-блок:** §8.0 запрещает forward stack-DoS/numeric-lossy на критическом пути — фиксируем как нормативные инварианты.
- **Q-closures:** **Q (effect?)** — RESOLVED: **PURE** (§9). **Q (auto vs opt-in)** — RESOLVED §3.0 Q1: **`#impl`-opt-in** (симметрия семейства, НЕ default-on). **Q (Serializer shape)** — RESOLVED Q3/Q12: generic-bound driven-pull. **Q (None-кодирование)** — RESOLVED Ф.4: absent по умолчанию (skip None, serde-default), `#serde` opt-in для explicit-null. **Q (unknown-field default)** — RESOLVED Q5: ignore (opt-in `deny`). **Q (sum-synth)** — RESOLVED Ф.2-sum: ГЕЙТ `[M-126-sum-*-rich]`/180.2. **Q (container blanket)** — RESOLVED Q13: compiler-side mono (`[M-161]`-обход). **Q (depth/numeric)** — RESOLVED Q14/Q15/D346.
- **docs/* (новые):** **`docs/serde.md`** (data-model + протоколы + record-auto-derive-контракт + атрибуты-таблица + tagging + cross-lang §2 + §1a + JSON-backend + soundness-инварианты); `docs/idioms/serde-json.md` (рецепты `json.encode`/`decode`/`rename_all`/`flatten`). Amend auto-derive-главу (рядом с `==`/`clone`/`hash`, отметить `#impl(Serialize)` как члена семейства + честную sum-гейт-ноту).

---

## 6. Миграция

- **`std/encoding/json` — НЕ переписывается, остаётся dynamic-backend.** `JsonValue`/`Json.parse`/`@into()`/`pretty`/getters/`ParseJsonError`/strict-`DuplicateKey` — **стабильный API, нетронут**. serde строит **typed-слой НАД ним** (Ф.4 reuse), не дублирует RFC-8259. **Два режима сосуществуют:** dynamic (`JsonValue` — что Plan 178 `.json()->JsonValue` приземляет СЕЙЧАС) и static (`json.decode[T]` — что Plan 178 `.json[T]()` гейтит на этот план). Bridge `to_value`/`from_value` (Ф.4) связывает оба.
- **Возможный prereq в `std/encoding/json`** (Ф.0-аудит): если `Json.parse` **не** имеет собственного depth-bound (Q14 stack-DoS на парс-стороне) → добавить depth-guard в парсер ИЛИ обернуть `Json.parse` в bounded-driver. Координируется с std/encoding owner. Аналогично raw-numeric-token (Q15) — если `Json.parse` не выдаёт сырой токен, arbitrary-precision откладывается в §11 (f64-`LossyInteger` честно отклоняет).
- **`ParseJsonError` ↔ `DeError`:** `json.decode` ловит `Json.parse`-ошибку и **оборачивает** в `DeError{kind: Syntax, source: Json(ParseJsonError), location}` (source-chain, D325 R5 forwarding-ок) — line/col json.nv не теряются. Type-mismatch на этапе `JsonValue → T` даёт `DeError` с **семантическим path**.
- **Существующий код через `JsonValue`+`match`/`as_*`** — работает без изменений. Новый код выбирает `json.decode[T]`. Постепенная миграция.
- **`#stable`-обязательства:** новые `Serialize`/`Deserialize`/`SerError`/`DeError`/`json.encode`/`decode` — `#unstable(since="0.1")` до закрытия Ф.5 (sum-tagging может уточнить wire-format sum-типов); **record-кодирование стабилизируется раньше** (после Ф.4), чтобы Plan 178 мог опереться на record-DTO. **rebuild `nova-cli`** после правок `.nv` (include_str! для `std/prelude/protocols.nv` если протоколы там + `std/encoding/serde/*.nv`) И после изменений auto-derive-синтеза в `compiler-codegen`.

---

## 7. Тесты

`nova_tests/serde/` (pos = folder-module `module nova_tests.serde`); `nova_tests/serde/neg/` (neg). **Mock-тест:** serde PURE (без effect) → mock-триада **НЕ требуется** (§9; JSON-backend тоже pure str↔value — детерминизм по построению). Round-trip property как json.nv:948 (`decode(encode(v)) == v`).

**pos (round-trip + поведение):**
- **auto-derive record (Ф.2):** flat `{name str, age int}`; nested `{user User, tags []str}`; generic-поля `Vec[T]`/`Option[T]`/`HashMap[str,V]`; рекурсивный `Tree{val int, kids []Tree}`; **через `#impl(Serialize+Deserialize)`, нулевое тело** → round-trip, `decode(encode(v))==v`.
- **option/seq/map:** `Option[T]` None↔(absent\|null §3.0); пустой `[]`/`{}`; `HashMap[str,T]`; вложенные `Vec[Vec[int]]`.
- **numeric-fidelity (Q15):** small int round-trip exact; 2^53 граница; negative int round-trip.
- **depth (Q14):** вложенность <128 round-trip OK.
- **bytes (Q9):** `[]u8`↔base64-строка round-trip.
- **атрибуты (Ф.3b):** `rename`/`rename_all`(camelCase↔snake_case round-trip); `skip`+`default`; `flatten`-inline; `default=expr` при missing.
- **bridge/dynamic-coexist:** `to_value`/`from_value`; `Json.parse(json.encode(v))!! == json.to_value(v)` (Ф.4 e2e); смешанный static+`JsonValue`-поле.
- **deser-error path/location:** type-mismatch → `DeError` с непустым `path` (`expected int, found str at $.user.tags[2]`); `MissingField` с path; `UnknownField` под `deny_unknown_fields` с path; syntax-error chain в `ParseJsonError` (line/col не потеряны).
- **(гейт Ф.2-sum/Ф.5):** sum unary/payload/struct-вариант externally-tagged; 4 tagging-режима; untagged first-match — **в pos ТОЛЬКО после разгейчивания `[M-126-sum-*-rich]`**.
- **`*_slow.nv`:** large-payload round-trip (≥10⁴ `Vec[Record]`); deep-nested под лимитом (Tree глубина ~100) — вне дефолт-сэмпла.

**neg (`neg/`, EXPECT_*):**
- **non-serializable → `EXPECT_COMPILE_ERROR`:** record с must-consume/resource-полем (E_SERDE_FIELD_NOT_SERIALIZABLE, **поле названо**); raw-pointer-поле; `HashMap[int,_]` → E_SERDE_NONSTRING_MAP_KEY (Q16); **silent field-drop НЕ происходит** (pos-control: все поля в выводе).
- **атрибут-ошибки → `EXPECT_COMPILE_ERROR`:** опечатка ключа (E_SERDE_BAD_ATTRIBUTE); `skip`(deser) без `default` (E_SERDE_SKIP_WITHOUT_DEFAULT); невалидный `rename_all`-литерал; `flatten`+`deny` (E_SERDE_FLATTEN_DENY_CONFLICT); (sum-гейт) `internal`-tag на tuple-варианте (E_SERDE_INTERNAL_TAG_NON_STRUCT).
- **D325-конвенция → `EXPECT_COMPILE_ERROR`:** `Fail[E]` для своей ошибки (R5); `try_`-метод не-from-сиблинг (R3).
- **runtime deser (match-on-Err / EXPECT_RUNTIME):** `deny_unknown_fields`+unknown → `UnknownField` с path; `MissingField` без `default`; type-mismatch → `UnexpectedType` с path; truncated JSON → `Syntax` (source-chain); **2^53+1 в int → `LossyInteger`** (НЕ silent); negative→uint → `OutOfRange`; **deep-nested (>128) → `DepthLimitExceeded`** (typed, НЕ stack-overflow-crash) — `*_slow.nv`.

---

## 8. Критерии приёмки

**§8.0 — ОБЯЗАТЕЛЬНО: без упрощений, как для прода.** Критический путь = record-auto-derive + JSON-backend (гейт Plan 178). Ни одного «решим потом» на нём. Конкретно:
1. **Record-auto-derive КОРРЕКТЕН для ВСЕХ форм record** — flat/nested/generic/рекурсивный; `Vec`/`Option`/`HashMap[str,_]`/скаляры. Не «MVP на flat-record» — полное структурное покрытие, round-trip-тесты на каждой форме. **Sum-auto-derive — НЕ на критическом пути 182, честно ГЕЙТНУТ** `[M-126-sum-*-rich]`/180.2 (не «решим потом» замаскированное под reuse — явный named-prereq).
2. **НИКАКОГО silent field-drop.** Каждое record-поле либо в выводе, либо явно `#serde(skip)`. Тип с не-сериализуемым полем → **E_SERDE_FIELD_NOT_SERIALIZABLE** (compile-time, поле названо). Тест «все поля присутствуют» + neg на drop.
3. **Deser-ошибки ЛОКАЛИЗОВАНЫ** — `DeError` несёт `path`/`expected`/`found`; syntax-chain сохраняет `ParseJsonError` line/col. Не «строка без контекста».
4. **Soundness-инварианты (D346) ПРИЗЕМЛЕНЫ, НЕ forward'нуты:** (a) **depth-guard** (Q14) — adversarial-вложенность → typed `DepthLimitExceeded`, НЕ crash/DoS (neg-тест); (b) **numeric** (Q15) — 2^53+1/negative→uint → typed error, НЕ silent-lossy (neg-тест); (c) **map-keys** (Q16) — `HashMap[non-str,_]` → compile-error. **§8.0 явно запрещает forward stack-DoS/numeric-lossy на критическом пути — эти решения ПРИНЯТЫ, не отложены.**
5. **Container-conformance — честно compiler-side** (Q13, `[M-161]`-обход), НЕ blanket-`.nv`-фикция. Если Ф.0 покажет, что часть выразима как `.nv` — задокументировать; иначе всё в синтезаторе.
6. **Каждая behavior-change несёт pos+neg + аргумент звучности** на КАЖДОЙ приземлённой фазе. **Честный GATE:** Ф.2-sum/Ф.5 (sum+tagging) «позже», гейт `[M-126-sum-*-rich]` (Plan 178 record-DTO не требует). **Честный scope-out:** TOML/binary/zero-copy/schema-gen/runtime-cycle/key-codec/arbitrary-precision → §11.
7. **Конвенции:** D325 (Result-everywhere, R5 `Fail[E]`-запрет neg-тест); PURE (без effect — обосновано); test-conventions (pos folder-module / neg `neg/`); nv-sourcing (serde-логика+JSON-backend в `.nv`, reuse существующего json `.nv`; auto-derive-synth в компиляторе — необходимо).

**Per-phase:**
- **Ф.0:** §3.0 (Q1–Q20) закрыт; D340–D346 spec-first; record-auto-derive-seam локализован; **sum-synth-OPEN + `[M-161]`-OPEN + attribute-no-attrs verified и зафиксированы как гейты/Ф.3a-объём**; depth/raw-token-аудит `Json.parse`; D-renumber verified; gap-нота если 177/178/179 не в spec.
- **Ф.1:** протоколы+data-model+`SerError`/`DeError` компилируются (generic-bound нотейшн Q12); ручной impl round-trip на toy-record; `DeError.path` непустой; R5-neg fires.
- **Ф.2:** record-auto-derive синтезирует для **всех** форм record через `#impl`; round-trip pos на каждой форме; **E_SERDE_FIELD_NOT_SERIALIZABLE** neg (поле названо); E_SERDE_NONSTRING_MAP_KEY neg; no-silent-drop тест зелёный; container-mono работает.
- **Ф.3a:** `#serde`-парсинг+AST-storage+E_SERDE_BAD_ATTRIBUTE; field/variant/type-attr-storage существует.
- **Ф.3b:** все атрибуты потребляются synth'ом; статическая валидация (E_-коды) neg-покрыта; unknown-policy=ignore default, `deny` opt-in.
- **Ф.4:** `json.encode`/`decode`/`encode_pretty`/`to_value`/`from_value`/`decode_bytes`/`decode_with` round-trip; e2e-bridge; deser-error с path; **depth-guard + numeric-fidelity neg зелёные**; **🔓 контракт `json.decode[T]`/`encode[T]` готов для RECORD-DTO → Plan 178 Ф.json РАЗГЕЙЧЕН (record)** (явный sign-off в README/182 §HARD-GATE; sum-DTO остаётся sub-gate на Ф.2-sum).
- **Ф.2-sum (когда разгейчено `[M-126-sum-*-rich]`):** sum-auto-derive все формы; externally-tagged wire; no silent variant-drop.
- **Ф.5 (когда разгейчено):** 4 tagging-режима round-trip; struct-only-инвариант + untagged-footgun документированы; neg на невалидные комбинации.

---

## 9. Конвенции + координация

**Конвенции (refs):** module-conventions (folder=один модуль `std.encoding.serde`; scalars=value-record D215; **byte-first** где есть bytes-model; **PURE codec — БЕЗ effect** и БЕЗ effect-триады: serde не делает I/O, оперирует values/bytes; JSON-backend тоже pure str↔value — явное PURE-исключение из «effect-subsystems = TRIAD», как module-conventions предписывает для pure codec/serde; **nv-sourcing:** serde-логика+JSON-backend в `.nv`, reuse существующего `std/encoding/json` `.nv`, БЕЗ нового C-FFI; **auto-derive-synth — в компиляторе** `compiler-codegen` (необходимо, как `@clone`-synth)). **D325** (R1 fallible→`Result[T,SerError]`/`Result[T,DeError]`; R2 bare-имя `encode`/`decode`/`serialize`; R3 `try_` только для from-сиблинга; R4 `Option`=genuine absence; **R5 `Fail[E]` запрещён** для своих ошибок, ок forwarding `ParseJsonError`/`Utf8Error`/`Base64Error`). **consume** D131/D133/D180 (sub-serializers `Seq/Map/Struct` = must-consume `end()`-гарантия; serde-VALUES НЕ must-consume — pure; ОБРАТНОЕ: must-consume-тип → **E_SERDE_FIELD_NOT_SERIALIZABLE**). **test-conventions** (EXPECT_*; pos folder-module / neg `neg/`; **mock НЕ требуется** — PURE; slow=`*_slow.nv`). conventions-governance: изменения только по согласованию.

**🔑 COMPILER auto-derive — работа для ИМПЛЕМЕНТОРА (не код в плане).** Ф.2 (record)/Ф.3b/Ф.2-sum/Ф.5 — синтез в **компиляторе** (`compiler-codegen/src/codegen/emit_c.rs`, рядом с `@clone`/`@equal`-synth). План **специфицирует контракт + emitted-shape** (D341/D342/D343/D345), это **НЕ сам код**. **Честная граница:** record-synth = reuse существующей memberwise-машины (✅ landed); sum-synth = НОВАЯ инфра (`[M-126-sum-*-rich]` OPEN) → гейт/180.2; container-conformance = compiler-side mono (`[M-161]` OPEN) НЕ blanket-`.nv`; attribute-storage = НОВЫЙ AST/parser-объём (Ф.3a). Ф.3a-парсер расширяет `#visible_to`-механику (parser/mod.rs:4315).

**Координировать:**
- **🔓 Plan 178 (std/http)** — serde **ГЕЙТ для 178 Ф.json (record-DTO)**: typed `.json[T]()`/`.json(v T)` жёстко гейтятся на `json.decode[T]`/`encode[T]` ([178:9/217/428](178-std-http.md), Q20). **Ф.4-релиз = sign-off** в README + 178 §HARD-GATE для **record-DTO**. **dynamic `.json()->JsonValue`** в 178 приземляется СЕЙЧАС над существующим json (не ждёт serde). **sum-DTO** в HTTP (редко) → sub-gate на Ф.2-sum/180.2 (явно отметить в 178, что record-DTO достаточно для MVP).
- **`std/encoding/json`** — reuse `JsonValue`/`Json.parse`/`@into()`/`pretty`/`format_num`/strict-`DuplicateKey`; **НЕ переписывать** (§6); `ParseJsonError`→`DeError` source-chain. **Возможный prereq:** depth-bound в `Json.parse` (Q14) + raw-numeric-token (Q15) — Ф.0-аудит, координация со std/encoding owner.
- **`std/encoding/base64`** — `bytes`↔base64 (Q9); `Base64Error`→`DeError`-source. Explicit-import.
- **Auto-derive-машина (D109 amend + D230)** — record-memberwise-seam, в который встраивается serde-record-synth (✅ landed); **`[M-126-sum-*-rich]`** — sum-rich-derive prereq для Ф.2-sum.
- **`[M-161-parametric-return]`** — blanket-parametric-return OPEN; container-conformance обходит через compiler-mono; если 161-parametric landed позже — часть container-impl можно перенести в `.nv` (followup).
- **Plan 177 (D325)** — Result-everywhere (conformant by-construction; R5-neg-тест).

После большой задачи — обновить `project-creation.txt` + `nova-private/discussion-log.md` + `simplifications.md`.

---

## 10. Фоновые агенты

- **НЕ `git stash`** — репо-глобален, worktree делят `.git` → collision/потеря (feedback-worktree-shared-stash). Baseline для diff/regress = **temp-worktree** или **commit+reset**, не stash.
- **Rate-limit resilience:** фазы **resumable/идемпотентны** (Ф.0–Ф.5 commit-per-phase, no-amend); малые батчи; `agent()` null-tolerant; падение на rate-limit — фаза перезапускаема с последнего коммита.
- **`git add` ТОЛЬКО по именам файлов** (никогда `-A`/`.` — рядом другие агенты, feedback-git-add-specific); перед commit — **`git diff --cached --stat`** (в индексе могут быть чужие pre-staged правки, feedback-verify-index-before-commit).
- **БЕЗ `Co-Authored-By: Claude`** trailer (feedback-no-claude-coauthor; hook стоит, но смотреть руками).
- **Подтверждение перед background-`Agent`** (`run_in_background: true`) — feedback_no_background_agents.
- **`nova test` батчами <10 мин** (потолок таймаута 10 мин; полный прогон дробить — project-bash-timeout-10min-max); per-fix verify = targeted fixture, full только в конце фазы (feedback_targeted_test_per_fix). **nova_tests СЛОМАН — не гейт корректности** (feedback-nova-tests-not-correctness-gate): гейт = свои serde-фикстуры + detect172-стиль pos/neg + аргумент звучности.
- **rebuild `nova-cli`** после правок `.nv` (include_str!) И после изменений auto-derive-синтеза в `compiler-codegen` (иначе старый бинарь не видит новый синтез/протоколы).
- **Изолированный worktree `nova-p184`** сразу (feedback-isolated-worktree / worktree-naming); cd-префикс в каждой Bash-команде (cwd=main, feedback_worktree_cwd_clarity); env `NOVA_GC_LIB_DIR`/`INCLUDE_DIR` на main repo, libuv-submodule копировать (project-worktree-nova-test-setup).

---

## 11. Followup

- **`[M-180.2-sum-auto-derive]`** — sum-auto-derive + enum-tagging-strategies (Ф.2-sum/Ф.5) как **под-план 180.2**, гейтнутый на закрытии `[M-126-sum-equal-rich]`/`-clone-rich`/`-hash-rich` (match-arm-per-variant synth — НОВАЯ инфра, не reuse). Externally-tagged default + internal/adjacent/untagged + буфер-машина (Q17). НЕ на критическом пути Plan 178 (record-DTO достаточно).
- **`[M-180-backends-toml-yaml-binary]`** — TOML/YAML/binary(CBOR/MessagePack/bincode)-backends над теми же `Serialize`/`Deserialize`-протоколами (data-model формат-agnostic — построено Ф.1 ровно для этого; новый backend = новый `Serializer`/`Deserializer`-impl, auto-derive не трогается; binary даёт lossless int>2^53). «потом».
- **`[M-180-zero-copy-borrow]`** — zero-copy/borrow-deserialize (serde `#[serde(borrow)]`-аналог: `&str`/`[]u8`-срезы из буфера без копии). Требует lifetime/borrow-модель Nova на deser-входе (`str`=value-record D139/D215 — borrow-семантика требует lifetime-инфры); крупная фича. **Честный scope-out** (сейчас owned). «потом».
- **`[M-180-arbitrary-precision-numbers]`** — arbitrary-precision number-буфер (serde `Number`/Go `json.Number`-аналог) для lossless int>2^53 в JSON — требует raw-numeric-token из `Json.parse` (Ф.0-аудит). Пока — честный `LossyInteger`-reject (Q15). «потом».
- **`[M-180-runtime-cycle-detection]`** — runtime ref-cycle-detection на serialize (`@JsonIdentityInfo`-аналог; heap-indirect граф с обратной ссылкой). Пока depth-guard (Q14/§3.4a) превращает overflow в typed `DepthLimitExceeded` (как serde, который overflow'ит — мы строго лучше: typed-error). «потом».
- **`[M-180-nonstring-map-keys]`** — key-codec-протокол для `HashMap[int,_]`/`HashMap[Enum,_]` (serde `MapKeySerializer`-аналог, stringify/parse ключей). Пока — только `HashMap[str,_]`, прочее → compile-error E_SERDE_NONSTRING_MAP_KEY (Q16). «потом».
- **`[M-180-schema-gen]`** — schema-gen из типов (JSON-Schema/OpenAPI из auto-derive-метаданных — компилятор знает структуру; обратное к Go-reflection). Поддерживает Plan 178 OpenAPI-тулинг. «потом».
- **`[M-180-versioning-migration]`** — versioning/migration (`#serde(alias=...)` для эволюции полей, default-driven forward-compat, migration-хуки). «потом».
- **`[M-180-custom-impl-escape-hatch]`** — escape-hatch ручного `Serialize`/`Deserialize` (override auto-derive для newtype-обёрток/кастомного wire-format), параллельно `#impl` (как serde manual-impl; D77 user-wins уже даёт основу). Verify сосуществование auto-opt-out + ручной impl. «потом».