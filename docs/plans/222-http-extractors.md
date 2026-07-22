<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# План 222 (зонтик) — nova-http до Axum-паритета: audit-driven переработка, не тонкий слой

**Статус:** 📋 ЗОНТИК, ДИЗАЙН, НЕ СОГЛАСОВАН (реструктурирован после ревью владельца 2026-07-22:
«ServeMux нулевой — брать из Axum, возможно удалить; пройтись по ВСЕМ блокам; если хуже — делаем
по образу Axum; serde использовать по максимуму и докрутить до оригинала»). Ждёт owner-go на
под-планы.
**Приоритет:** ниже релиза 221. **Пакет:** `nova-http` (внешний git-репо) + компилятор (для serde).
**Родитель:** [178](178-std-http.md) (http umbrella) + [180](180-serde-derive.md) (serde — 222.2 = его
продолжение).

## 0. Аудит текущего http против Axum (факты из кода, ревью 2026-07-22)

Прежняя редакция плана самонадеянно приняла существующее за «хороший фундамент» и предлагала тонкий
extract-слой. Аудит показал — местами фундамент дырявый:

| Блок | Что РЕАЛЬНО (код) | vs Axum | Решение |
|---|---|---|---|
| **Routing (ServeMux)** | линейный `for r in routes` O(n)/запрос; `{param}` ТОЛЬКО 1 сегмент; НЕТ `{*wildcard}`; **first-match по порядку регистрации** (не Go-1.22-precedence — док врёт); нет групп/nesting; нет middleware | trie-router, wildcards, precedence, nested, MethodRouter, layers | **ПЕРЕДЕЛАТЬ С НУЛЯ по Axum (222.1); ServeMux ретайр/фасад** |
| **Handler** | `fn(ServerRequest)->ServerResponse` newtype (server.nv:329) | ровно то, во что Axum-Handler стирается | **фундамент ОК, оставить**; эргономика — поверх |
| **ServerResponse** | text/html/bytes/empty/redirect/stream/sse/header | приличный набор, но нет `IntoResponse`, нет `.json(T)` | **расширить (222.5), не сносить** |
| **serde** | derive record/sum + tag-режимы; формат — JSON | **НЕТ rename/rename_all/skip/default/flatten** (180 их спроектировал, не дошил) | **докрутить до Rust-паритета (222.2)** |
| **run-loop / servernet** | НЕ аудирован в этом заходе | — | **отдельный аудит перед тем как утверждать «годен»** (222.6, разведка) |

## 1. Разбивка на под-планы + параллелизм (ключевое требование владельца)

Зоны нарочно НЕ пересекаются → максимум параллельных волн:

```
ВОЛНА A:
  222.1  Router с нуля (Axum-style)        — nova-http/src/*.nv
  222.2  serde field-атрибуты = **[180.1 Ф.1/Ф.7/Ф.10](180.1-serde-parity-and-beyond.md),
         УЖЕ В РАБОТЕ** (волна p180-serde-field-attrs, 2026-07-22: rename/rename_all/skip/
         default/alias/flatten + strict-by-default unknown-fields + wire-валидация) —
         здесь НЕ дублировать, только ссылка

ВОЛНА B (после A):
  222.3  extractors + IntoResponse          — nova-http .nv (нужен новый Router + serde-attrs)
  222.5  ServerResponse-расширение (.json,   — nova-http .nv (мелкий, свой файл)
         IntoResponse-конструкторы)

ВОЛНА C (после B):
  222.4  middleware / layers / группы        — на новом Router

РАЗВЕДКА — ДО ВОЛНЫ B (ревью 2026-07-22: «параллельно, не блокирует» опасно — если
аудит найдёт, что run-loop надо переделывать (как нашёл про ServeMux), это перечеркнёт
222.3/222.4 поверх; дешёвая разведка идёт ПЕРВОЙ вместе с волной A):
  222.6  аудит run-loop/servernet vs Axum run — отчёт → решение до старта B
```

**Критический путь:** 222.1 ∥ 222.2 → 222.3. Всё остальное навешивается. 222.2 — прекондишн реальной
пользы extractors (без `rename_all` camelCase↔snake_case любой реальный JSON-API мимо).

---

## 2. Под-план 222.1 — Router с нуля (Axum-модель)

**ServeMux ретайрится** (или остаётся как `Router.flat()`-совместимость-фасад — решить на Ф.0).
Новый `Router`:
- **Segment-trie** (не линейный скан): узлы — литерал / `{param}` / `{*rest}`-catch-all; O(глубина пути),
  не O(число маршрутов).
- **Wildcards:** `{name}` (один сегмент) + `{*name}` (хвост-catch-all, Axum-паритет).
- **Precedence:** статический сегмент > `{param}` > `{*rest}` (Axum/Go-1.22 specificity — конкретный
  побеждает, НЕ порядок регистрации).
- **MethodRouter:** `Router.route(path, get(h).post(h2))` — метод-диспатч как отдельный композируемый
  объект (Axum-модель), 405+Allow из него.
- **Nested:** `Router.nest("/api", sub_router)` — под-роутер с префиксом (замена слабых «групп»).
- **Fallback:** `Router.fallback(h)` — цепочка, не одинокий `not_found`.
- **Типизированные params** отдаёт слой 222.3 (Router даёт сырые `[](str,str)`, extractors типизируют).
- **Route-конфликт = ошибка регистрации** (ревью 2026-07-22): дубль-маршрут/пересечение
  precedence → typed `Result`-ошибка при `route()` (Axum ПАНИКУЕТ — мы лучше: typed, а где
  статически выводимо — компайл-диагностика).
- **`MethodRouter.fallback`** per-route (405-семантика на уровне метода-набора, не только
  глобальный `Router.fallback`) — Axum-паритет.

Всё — обычный `.nv` в nova-http; синтаксис Nova не меняется. D-амендмент не нужен (пакет-уровень).

---

## 3. Под-план 222.2 — serde до Rust-паритета (продолжение 180, КОМПИЛЯТОР)

**Диагноз (факт):** 180 спроектировал полный attr-набор (180-план line 90: «rename/rename_all/skip/
default/flatten — эталон»), но `SerdeArg` (ast/mod.rs) довёз ТОЛЬКО `Tag`/`Content`/`Untagged`
(sum-tagging). Field-атрибуты — не реализованы. `#serde(rename=…)`-обещания в 180 §68 аспирационны.

**Довезти (auto_derive.rs synth + SerdeArg + attr-парсер), приоритет по web-ценности:**
| Атрибут | Уровень | Зачем (web) | Ценность |
|---|---|---|---|
| **`rename_all = camelCase`** | container | фронт шлёт camelCase, поля Nova snake_case — **table-stakes** | 🔴 без него extractors бесполезны |
| **`rename = "..."`** | field | точечное имя поля | 🔴 |
| **`default`** | field | опциональные поля запроса | 🔴 |
| **`skip` / `skip_serializing_if`** | field | не слать null/пустое | 🟠 |
| **`alias`** | field | принять несколько входных имён | 🟠 |
| **`flatten`** | field | слить вложенный record (pagination-обёртки) | 🟠 |
| `deny_unknown_fields` | container | строгий вход | 🟢 (частично есть по 180 §87) |

**Где Nova может СДЕЛАТЬ ЛУЧШЕ Rust-serde (не только паритет — требование владельца «что можно лучше»):**
1. **`rename_all` типизированным enum**, не magic-строкой: `#serde(rename_all: CamelCase)` — опечатка
   `"camleCase"` у Rust компилируется молча, у нас → `E_SERDE_BAD_ATTRIBUTE` (180 §68 обещал — здесь
   реализуем по-настоящему).
2. **`default` через языковые field-defaults**, не Rust-`Default`-trait-пляску: у Nova уже есть
   default-арги (`new(cap=0)`); если довести до record-полей — `default` берёт значение поля напрямую,
   единообразно, без отдельного trait.
3. **Compile-time валидация ВСЕХ атрибутов** (auto_derive — не proc-macro): опечатка ключа/несовместимость
   (`flatten` на скаляре) → внятный компилятор-диагноз, не макро-развёртка-мусор.
4. **Field-path в ошибках**: `DeError` структурный (180) → `"users[3].createdAt: expected int, got str"`
   вместо serde-строки. Довести path-трекинг.
5. **`flatten` проще**: JSON-only → нет serde-ограничения «flatten ломается на non-self-describing
   форматах».

D-амендмент к D382 (serde-attrs) — язык-меняющее (новые `#serde(...)`-формы) → в том же слиянии.

---

## 4. Под-план 222.3 — extractors + IntoResponse (поверх 222.1/222.2)

`Serialize`/`Deserialize` — ЕДИНСТВЕННЫЕ compiler-known протоколы здесь (auto_derive.rs синтезирует их
тела). `FromRequest`/`IntoResponse` — ОБЫЧНЫЕ `.nv`-протоколы, компилятор их не знает.

**Граница serde компилятор/nv (факт):** в Rust-компиляторе зашит ТОЛЬКО синтез тел
`@serialize`/`.deserialize` + разбор `#serde(...)`. Протоколы `Serializer`/`Deserializer`, скаляр-
conformance, весь JSON-формат (`JsonSerializer`/`JsonDeserializer`, json.nv) — обычный `.nv` (1188
строк). Значит extractor-источники — 0 строк Rust.

**Механика — extractors переиспользуют serde максимально (требование владельца):**
`Path[T]`/`Query[T]` — не новые парсеры, а новые `Deserializer`-ИСТОЧНИКИ для того же `T.deserialize`:
```nova
export type Path[T] value { data T }
#impl(FromRequest)
fn Path[T Deserialize] @from_request(req ServerRequest) -> Result[Path[T], HttpError] {
    mut d = ParamsDeserializer.at(req.params)   // реализует Deserializer над [](str,str)
    match T.deserialize(d) { Ok(v) => Ok(Path{data: v}), Err(e) => Err(HttpError.decode_error(e.to_str())) }
}
```
**Реальный serde-API источника** (serde.nv:124-165, НЕ вымышленный): навигация по имени
`@enter_field(key) -> Result[Self, DeError]` + чтение по типу `@deser_int()`/`@deser_str()`/… (без
имени). Синтез генерит `d.enter_field("id")?.deser_int()?` — имя выбирает ОТКУДА, тип КАК. Значит
`ParamsDeserializer`/`QueryDeserializer` реализуют ровно эти методы над плоским списком/query-строкой.
`T` (твой `UserIdParams`/`CreateUserRequest`) синтезируется `#impl(Deserialize)` ОДИН раз, работает со
всеми источниками (JSON-тело / path / query).

**Протоколы:**
```nova
export type FromRequest protocol { static fn from_request(req ServerRequest) -> Result[Self, HttpError] }
export type IntoResponse protocol { fn into_response() -> ServerResponse }
```
`Result[R IntoResponse, E IntoResponse] : IntoResponse` (бланкет) → `?`-эргономика в хендлерах
бесплатно. **Синтаксис Nova не меняется** — обёртки это обычные value-record с полем `data`, доступ
`req.data.field` (НЕ Rust `Json(input): Json<T>` pattern-in-parameter — такого в Nova нет).

**Встроенные:** `Path[T]`/`Query[T]`/`Json[T]`(над `json_decode_body`)/`Bytes`/`Text`/`Headers`.
**Регистрация** — арность-overload'ы 0..4 на новом Router (прецедент 138.4 full-sig mangle; ловушка
`[M-concrete-instance-arity-overload-mangle]` из П21 — если стрельнёт, стоп+отчёт, не обход).
**State/DI — closures, НЕ Axum-`State<T>`-erased-map** (компайл-тайм-проверено, забыл зависимость →
ошибка компиляции, не рантайм-паника).

---

## 5. Под-план 222.4 — middleware / layers / группы

`Handler` = `fn(req)->resp` → middleware = ф-ция высшего порядка `fn(next Handler) -> Handler`
(Express-`use`-дух, без отдельного трейта). `Router.layer(mw)` копит цепочку, оборачивает Handler при
регистрации (не при dispatch — дешевле). Группы уже покрыты `Router.nest()` (222.1).

## 6. Под-план 222.5 — ServerResponse-расширение

`IntoResponse` для `str`/`ServerResponse`/`Json[T Serialize]`/`Result[R,E]`-бланкет; прямой
`ServerResponse.json[T Serialize](status, T)` конструктор (сейчас json только через serdejson вручную).

## 7. Под-план 222.6 — аудит run-loop/servernet (разведка)

Отдельный отчёт: connection-handling / graceful shutdown / keep-alive / timeout-drain vs Axum-run
(hyper). Пока НЕ утверждаем «годен» — [M-187]-семья намекает на M:N-сложности под нагрузкой. По отчёту —
решение «оставить/усилить/переделать», отдельным под-планом.

## 8. Маркеры (регистрируются этим планом)

- `[M-serde-field-attrs-unimplemented]` (P2) — rename/rename_all/skip/default/flatten спроектированы в
  180, `SerdeArg` довёз только tag/content/untagged; блокирует реальную web-пользу → 222.2.
- `[M-json-serializer-set-pending-naming]` (P3) — `JsonSerializer @set_pending` (json.nv:71) нарушает
  property-конвенцию (`set_`-префикс); fallible (`->Result`) → не чистый property-сеттер, а реализация
  `@struct_field` — инлайн/переименовать без `set_`.
- `[M-servemux-routing-placeholder]` (P2) — linear-scan + 1-сегмент-param + no-precedence; закрывается
  222.1 (Router с нуля).

## 9. Гейты / модель

Каждый под-план — свои таргетные тесты + `nova test` затронутого модуля δ0; 222.2 — гейты в
180.1; полный пакетный прогон nova-http — интегратор. **Интеграционный гейт всего 222
(ревью 2026-07-22): флагман aggregator (живой потребитель nova-http) собирается
--strict-effects + держит loadtest.ps1 (68 блоков) + полный мега-CU conformance — на КАЖДОМ
вливании под-плана.** Модель: 222.1/222.3/222.4/222.5 — sonnet (nova-http .nv по карте);
222.6 — opus-разведка. Приёмка/слияние/флип-гейты — интегратор.

## 10. Очерёдность go (зафиксировано 2026-07-22)

Первый owner-go после тегов v0.1 = **222.1 (Router) ∥ 222.6 (run-loop аудит)**; 222.2 уже
идёт как 180.1. Волна B (222.3+222.5) — после обоих A-результатов И вердикта 222.6.
