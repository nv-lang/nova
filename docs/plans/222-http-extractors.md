<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# План 222 — Extractors для `nova-http`: типобезопасный routing-слой в духе Axum, без Rust-сложности

**Статус:** 📋 ДИЗАЙН, НЕ СОГЛАСОВАН (черновик по запросу владельца 2026-07-22: «план по реализации
такого модуля для Nova» — после сравнения ServeMux/chi/Express/Axum). Ждёт owner-go.
**Приоритет:** ниже релиза 221 (не блокер). **Пакет:** `nova-http` (внешний git-репозиторий,
НЕ core-язык — все механизмы ниже используют существующие generics/protocols, **новый синтаксис
Nova не нужен**, D-амендмент не требуется).
**Родитель:** [178](178-std-http.md) (`ServeMux`/`Handler` — базовая маршрутизация, уже приземлена).

## 0. Мотив (одной фразой)

`ServeMux` (Go-1.22-калька, план 178) даёт маршрутизацию, но не эргономику: параметры вынимаются
вручную (`req.param("id")` → `Option[str]` → ручной парсинг), тело — вручную (`json_decode_body[T]`
внутри хендлера), любая ошибка формата — ручной `match`. Хочется Axum-уровня удобства
(`async fn handler(Path(id): Path<u32>, Json(body): Json<CreateUser>)`), но **без Rust-цены**
(borrow checker, `async`-цветность, макросы для tuple-impl'ов).

**Ключевой инсайт (почему Nova может быть проще Axum, а не только красивее ServeMux):**
1. Nova M:N-фибры дают синхронный код без `async`-заразности — Axum extractors асинхронны
   из-за streaming-тел; у нас `ServerRequest.body` уже **полностью буферизован** (`[]u8`,
   server.nv:91) → извлечение данных **всегда синхронно**, `Result`, без futures.
2. У Nova уже есть generics/protocols первым классом (не макрос-эмуляция трейтов) —
   `[T1 FromRequest]`-bound на методе регистрации ничем не отличается от любого другого
   generic-bound в языке; программисту не нужно понимать отдельную «систему трейтов ради веба».
3. Result-everywhere (D325) — уже канон std; `?`-проброс ошибки экстрактора работает бесплатно.
4. State-инъекция в Axum (`Extension<T>`/`State<T>`) — type-erased `Any`-map с рантайм-паникой
   «type not found», общепризнанный wart. У Nova есть настоящие замыкания — `Handler`
   (server.nv:329) это `fn(ServerRequest) -> ServerResponse`, значит state захватывается
   **замыканием в момент регистрации**, компайл-тайм-проверено, без Extension вообще
   (см. §5 — здесь Nova можно сделать ПРОЩЕ Axum, не только приблизиться).

## 1. Архитектурная опора: extractors переиспользуют serde, не изобретают парсер

**Разграничение (уточнено ревью владельца 2026-07-22 — «serde зашит в компилятор?»):**
`Serialize`/`Deserialize` — ЕДИНСТВЕННЫЕ два протокола этого плана, которые компилятор ЗНАЕТ
по имени: `#impl(Deserialize)` над record'ом заставляет `auto_derive.rs`
(compiler-codegen/src/protocols/auto_derive.rs — тот же synth-механизм, что `Equal`/`Hash`/
`Clone`/`Display`/`Debug`, все 8 в списке `is_builtin_protocol`) СГЕНЕРИРОВАТЬ тело
`.deserialize` по полям типа by TYPE-DIRECTED PULL (скаляр → `deser_int`/…, вложенный record →
рекурсивный `T.deserialize`, `Option` → инлайн null-check) — это единственный способ обойти
поля пользовательского типа без рантайм-reflection (которого в Nova нет).
**`FromRequest`/`IntoResponse` (этот план, §2) — ОБЫЧНЫЕ пользовательские protocols, компилятор
их не знает и не синтезирует; вся логика — рукописный `.nv`-код в `nova-http`.**

Extractors НЕ трогают serde-derive-синтез — только ЗОВУТ уже готовый `T.deserialize`, подсунув
СВОЙ источник данных: `nova-http` уже несёт `Deserialize`-протокол
(std/src/encoding/serde/serde.nv:187) и рабочий `json_decode_body[T Deserialize]`
(serdejson.nv:37) поверх абстрактного `Deserializer`-протокола (источник данных отделён от
`T.deserialize` по конструкции serde — Plan 180). Вместо отдельного парсера для path-params и
query-string — **два новых `Deserializer`-имплементора**, оба кормят ТОТ ЖЕ `T.deserialize(d)`:

- `ParamsDeserializer` — оборачивает `[](str, str)` (уже поле `ServerRequest.params`) как
  плоский объект-источник;
- `QueryDeserializer` — парсит `a=1&b=2` (raw `ServerRequest.query`) в тот же плоский вид.

**Следствие:** `Path[T]`/`Query[T]` — это не два новых движка, а два новых **источника** для
ОДНОГО compiler-synthesized движка; `T` (твой `CreateUserRequest`/`UserIdParams`) синтезируется
`#impl(Deserialize)` РОВНО ОДИН РАЗ и работает со всеми тремя источниками одинаково.
Compiler-conventions §0/§10 («один путь») соблюдён по построению.

`Json[T]`/`Path[T]`/`Query[T]` сами `#impl(Deserialize)` НЕ несут (их поле `data T` — это `T`,
уже готовый serde-тип; сама обёртка в JSON/params не сериализуется напрямую) — они лишь
МАРШРУТИЗИРУЮТ, откуда `T.deserialize` возьмёт данные:
```nova
export type Json[T] value { data T }

#impl(FromRequest)
fn Json[T Deserialize] @from_request(req ServerRequest) -> Result[Json[T], HttpError] {
    match json_decode_body[T](req.body) {   // существующий serde-вызов (178/180), без изменений
        Ok(v)  => Ok(Json { data: v })
        Err(e) => Err(e)
    }
}

export type Path[T] value { data T }

#impl(FromRequest)
fn Path[T Deserialize] @from_request(req ServerRequest) -> Result[Path[T], HttpError] {
    mut d = ParamsDeserializer.at(req.params)
    match T.deserialize(d) {                // ТОТ ЖЕ T.deserialize, что у Json[T] — другой источник
        Ok(v)  => Ok(Path { data: v })
        Err(e) => Err(HttpError.decode_error(e.to_str()))
    }
}
```

## 2. Протоколы (все — вписываются в существующие generic/protocol-механизмы)

```nova
// Извлечение из запроса — ВСЕГДА синхронно (тело уже буферизовано).
export type FromRequest protocol {
    static fn from_request(req ServerRequest) -> Result[Self, HttpError]
}

// Хендлер может вернуть что угодно, что умеет стать ServerResponse —
// не только голый ServerResponse (Axum IntoResponse-паритет).
export type IntoResponse protocol {
    fn into_response() -> ServerResponse
}
```

`Result[R IntoResponse, E IntoResponse] : IntoResponse` — бланкет-impl (Ok → R.into_response(),
Err → E.into_response()) — это и даёт `?`-эргономику в хендлерах бесплатно.

**Важно про синтаксис (поправка после ревью владельца 2026-07-22):** в Axum-примерах параметр
деструктурируется прямо в сигнатуре — `fn h(Json(input): Json<T>)`. **В Nova такого паттерна
НЕТ и не будет** (не нужен) — обёртки-extractors это ОБЫЧНЫЕ value-record'ы с именованным полем,
без tuple-struct-магии:
```nova
export type Json[T] value { data T }
export type Path[T] value { data T }
```
Параметр хендлера — обычный типизированный параметр; распаковка — обычное чтение поля в теле:
```nova
fn get_user(req Path[UserIdParams]) -> Result[Json[User], HttpError] {
    ro db = get_db()                     // §5: захват через замыкание при регистрации, не параметр
    ro user = db.find(req.data.id)?      // HttpError уже IntoResponse — просто пробрасывается
    Ok(Json { data: user })
}
```

## 3. Встроенные extractor-типы

| Тип | Источник | Тело |
|---|---|---|
| `Path[T Deserialize]` | `req.params` через `ParamsDeserializer` | все `{name}`-капчи маршрута → поля T |
| `Query[T Deserialize]` | `req.query` через `QueryDeserializer` | query-string → поля T |
| `Json[T Deserialize]` | `req.body` | тонкая обёртка над существующим `json_decode_body[T]` |
| `Bytes` | `req.body` | сырые байты один-в-один (для non-JSON API) |
| `Text` | `req.body` | `body.to_str()` с `HttpError::Protocol` на невалидный UTF-8 |
| `Headers` | `req.headers` | прямой доступ без парсинга (уже `HeaderMap`, готов) |

Для однопараметрового маршрута (`/users/{id}` → нужен только `id`) — v1 требует record-обёртку
(`Path[{ id: int }]`)); удобная форма `Path[int]("id")` для голого скаляра — **V2-добавка**, не
блокер (см. §7).

## 4. Регистрация хендлеров — арность-generic overload'ы (без макросов)

Прецедент из ЯДРА языка: full-signature overload mangling для generic-type методов уже работает
(Plan 138.4 — `Vec @index`), default-арг statics резолвятся по арности (Plan 1/5 — `new(cap=0)`).
Регистрация экстрактор-хендлеров — тот же класс: небольшое ЗАКРЫТОЕ семейство перегрузок по
числу extractor-параметров (0..4 — Axum поддерживает до 16 через макрос, нам столько не нужно;
0/1/2/3/4 покрывает подавляющее большинство REST-хендлеров, замер — по факту миграции 187):

```nova
export fn ServeMux mut @get[R IntoResponse](path str, h fn() -> R) -> @
export fn ServeMux mut @get[T1 FromRequest, R IntoResponse](path str, h fn(T1) -> R) -> @
export fn ServeMux mut @get[T1 FromRequest, T2 FromRequest, R IntoResponse](path str, h fn(T1, T2) -> R) -> @
// ... до 4; @post/@put/@delete/@patch — то же семейство
```

Тело одной перегрузки (остальные — механическая копия по арности):
```nova
export fn ServeMux mut @get[T1 FromRequest, R IntoResponse](path str, h fn(T1) -> R) -> @ {
    @get(path, Handler.new(fn(req ServerRequest) -> ServerResponse {
        match T1.from_request(req) {
            Ok(v1) => h(v1).into_response(),
            Err(e) => e.into_response(),
        }
    }))
}
```
Существующая нетипизированная форма `@get(path, Handler)` (server.nv:369) **остаётся** — низкоуровневый
эскейп-люк, extractor-формы — сахар поверх неё, не замена (D9 не нарушается: разные арности —
не «две двери» к одному, а расширение существующей перегрузочной лестницы).

## 5. State/DI — closures, НЕ type-erased map (сильнее Axum, не слабее)

Canonical v1 (никакого нового механизма — это уже работает в Nova):
```nova
fn make_get_user(db Db) -> Handler {
    Handler.new(fn(req ServerRequest) -> ServerResponse { ... db.find(...) ... })
}
mux.get("/users/{id}", make_get_user(db_pool))
```
Компайл-тайм-проверено (забыл передать `db` — ошибка компиляции, не рантайм-паника «Extension
not found», как в Axum). Документировать как ПЕРВИЧНЫЙ паттерн. **Опционально V3** — эффект-based
DI (`Db`-эффект, `with Db = pool { ... }` вокруг `mux.dispatch`) для тех, кто не хочет explicit
factory-функции — рассмотреть только если реальный опыт использования попросит; не проектировать
заранее (YAGNI).

## 6. Группы маршрутов и middleware (chi/Express-паритет, отдельно от extractors)

- **Группы:** `mux.route(prefix str, build fn(mut ServeMux) -> ()) -> @` — вложенный саб-mux,
  мержится в родителя с префиксом (chi `r.Route()`-паритет). Механически просто: саб-`ServeMux`
  строится, префикс подставляется в его routes при merge.
- **Middleware:** `Handler` — просто `fn(ServerRequest) -> ServerResponse`, значит middleware —
  функция высшего порядка `fn(next Handler) -> Handler` (Express `app.use()`-дух, без отдельного
  трейта). `mux.use(mw) -> @` копит цепочку, применяется при `.get/.post/...` регистрации
  (оборачивает готовый Handler перед вставкой в routes) — не при dispatch (проще, дешевле).

## 7. Вне объёма V1 (сознательно, не забыто)

- Голый-скаляр `Path[int]("id")` без record-обёртки — удобство, не блокер, V2.
- WebSocket-extractor — отдельный протокол общения, не эта модель.
- Multipart/form-data extractor — самостоятельный парсер, отдельный под-план если понадобится.
- Effect-based DI (§5) — только по реальному запросу.
- >4 extractor-параметров — если реальный хендлер настолько разбух, это code-smell до extractors.

## 8. Фазы

- **Ф.0 Протоколы + `ParamsDeserializer`/`QueryDeserializer`** (sonnet): `FromRequest`/
  `IntoResponse` в новом `src/extract.nv` (nova-http); два Deserializer-имплементора над
  существующим `Deserializer`-протоколом (serde.nv); юнит-тесты деконструкции params/query в
  простые record'ы (без HTTP-контекста вообще — чистые парсер-тесты).
- **Ф.1 Встроенные extractors** (sonnet): `Path[T]`/`Query[T]`/`Json[T]`(обёртка)/`Bytes`/`Text`/
  `Headers`, каждый `#impl(FromRequest)`; `IntoResponse` для `str`/`ServerResponse`/`Json[T
  Serialize]`/`Result[R,E]`-блонкет. Тесты — мок `ServerRequest`-фикстуры (образец —
  server_test.nv).
- **Ф.2 Арность-регистрация** (sonnet, по карте §4): 0..4-арные `@get/@post/@put/@delete/@patch`
  перегрузки на `ServeMux`. **Ловушка (протокол «стоп+находка», не обход):** если резолв
  generic-overload по арности+bound-у споткнётся (класс из П21-исполнения:
  `[M-concrete-instance-arity-overload-mangle]`/`[M-static-multi-overload-chain-call-type]` —
  соседняя история mangle-коллизий на инстанс-перегрузках) — чекпоинт + отчёт интегратору, НЕ
  героический workaround.
- **Ф.3 Группы + middleware** (sonnet, по карте §6): `.route()`/`.use()`.
- **Ф.4 Миграция примера + доки**: один реальный роут флагман-агрегатора (187) переписывается
  на extractor-форму бок-о-бок со старой (обе формы валидны — не breaking change), `docs/http.md`
  += раздел «extractors», сравнительный пример «до/после» (ServeMux-ручной vs extractor).

## 9. Гейты

Таргетно на каждой фазе: юнит-тесты новых файлов + `nova test src/server` (nova-http) δ0 +
существующий флагман-роут (187 aggregator) не ломается под старой формой (обратная совместимость
обязательна — новая форма аддитивна). Полный пакетный прогон nova-http (`nova test src`) —
интегратор перед слиянием каждой фазы. D-амендмент не требуется (чистый std/package-уровень).

## 10. Модель исполнения

Дизайн (этот файл) — карта. Ф.0-Ф.3 — sonnet последовательно (каждая фаза — свой коммит/PR,
не одна волна: риск арность-mangle в Ф.2 оправдывает отдельный чекпоинт до Ф.3). Ф.4 — sonnet.
Приёмка/слияние/полный гейт — интегратор.
