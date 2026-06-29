# std/http — как сделать common-path проще (решительный ответ)

## Вердикт

Делаем. Common-path схлопывается с `with Http = real_http() { supervised { ... } }` + builder-пролога до **одного вызова без `with`** — паритет с `requests.get(url).json()` / `fetch(url)`, и при этом **все 4 инварианта сохранены**. Механизм не новый: это ровно `main() Io` runtime-root handler, который в spec уже работает (`spec/decisions/04-effects.md:3171`, `:5573`). Дефолт-install `real_http` — последняя незакрытая часть H2-долга, явно названная в плане как отдельный пункт (`178-std-http.md:1099`).

Граница, которую НЕ пересекаем: дефолт меняет **только то, чей handler стоит на корне стека**, а НЕ сигнатуры. `Http` остаётся в типе (`fn f() Http -> ...`). Это отделяет звучный default-install от небезопасного thread-local/contextvars-ambient (который прячет capability из типа и ломает reasoning + forbid + testability разом).

## Target: минимальная программа (после упрощений)

**GET → typed JSON → error, 6 строк:**

```nova
fn fetch_user(id int) Http -> Result[User, HttpError] =>
    http.get_json[User]("https://api.example.com/users/${id}")   // 1 вызов: GET→error_for_status→json

fn main() {
    match fetch_user(42) {                  // НЕТ with — root real_http авто-установлен (как Io)
        Ok(u)  => println("user: ${u.name}")
        Err(e) => println("http error: ${e.to_str()}")
    }
}
```

**BEFORE (текущий план):**

```nova
fn fetch_user(id int) Http -> Result[User, HttpError] {
    ro client = HttpClient.new()
    ro resp = client.get("https://api.example.com/users/${id}").send()?.error_for_status()?
    resp.json[User]()
}
fn main() {
    with Http = real_http() {                              // ← церемония H2-долга
        supervised deadline: Instant.now() + 10.sec {
            match fetch_user(42) { Ok(u) => ..., Err(e) => ... }
        }
    }
}
```

Эталон, который догоняем — Rust reqwest даёт верный **семантический** образец (Result-everywhere, строгий serde, `error_for_status`, `?`-propagation — всё совпадает с инвариантами Nova), а Python/Go/Node задают эталон **церемонии** (free one-shot + ambient default). Nova после упрощений = семантика reqwest + церемония requests + фибры вместо async (тут Nova уже впереди: ни `#[tokio::main]`, ни `.await`, ни `suspend`).

## Ранжированные упрощения

### 1. default-install root `real_http` (главный рычаг) — SAFE-WITH-CAVEAT

- **Что:** рантайм инсталлирует `real_http()` нижним кадром effect-стека на входе процесса — тот же механизм, что root-`Io` для `main() Io`. Скрипт не пишет `with`; handler уже на стеке.
- **Сигнатура:** не меняется. `export fn http.get(url IntoUrl) Http -> Result[HttpResponse, HttpError]` — `Http` остаётся в типе.
- **Ценность:** убирает **единственный** реальный overhead Nova против всех 7 пиров. Это 100% headline H2-долга.
- **Цена:** нужен runtime-bootstrap-хук (новая запись в root-stack, не новый языковой конструкт). Скрипт, который НЕ хочет сеть, теперь обязан явно `forbid Http {}` (раньше «нет `with`» = «нет сети»).
- **Вердикт:** SAFE-WITH-CAVEAT. Caveat нормативный (см. trade-off ниже): **в test/lib-edition root `real_http` НЕ инсталлируется**, иначе забытый `mock_http` тихо уходит в реальную сеть.

### 2. richer one-shots + `get_json[T]` / `post_json[T,B]` — SAFE-WITH-CAVEAT

- **Что:** полный verb-набор + typed-json-one-call.

```nova
export fn http.get(url IntoUrl)               Http -> Result[HttpResponse, HttpError]
export fn http.head(url IntoUrl)              Http -> Result[HttpResponse, HttpError]
export fn http.delete(url IntoUrl)            Http -> Result[HttpResponse, HttpError]
export fn http.post(url IntoUrl, body []u8)   Http -> Result[HttpResponse, HttpError]
export fn http.put(url IntoUrl, body []u8)    Http -> Result[HttpResponse, HttpError]
export fn http.patch(url IntoUrl, body []u8)  Http -> Result[HttpResponse, HttpError]
export fn http.get_json[T](url IntoUrl)             Http -> Result[T, HttpError]   // gate serde
export fn http.post_json[T, B](url IntoUrl, body B) Http -> Result[T, HttpError]   // gate serde
```

- **Семантика:** `get_json[T]` = `get(url)? .error_for_status()? .json[T]()`. `post_json[T,B]`: `B`→тело (CT `application/json`), `T`←ответ (запрос-тип ≠ ответ-тип — частый случай).
- **Ценность:** typed JSON одной строкой (паритет с reqwest `.json()` / Ktor `.body()`); полнота verb'ов (паритет fetch/requests).
- **Цена:** 8 тонких фасадов над глобальным lazy-`Once`-клиентом, нулевая новая транспорт-логика.
- **Вердикт:** SAFE-WITH-CAVEAT. **Два гейта-условия landing'а:** (1) `get_json` зашивает `error_for_status` — это инвертирует Q4-семантику (в `http.get` 4xx = валидный Response, в `get_json` не-2xx → `Err`, теряя error-body). Громко задокументировать + escape-hatch `http.get(url)?.json[T]()` для сырого не-2xx body. (2) `get_json`/`post_json`/`.json[T]` гейтнуты на serde (Plan 184) — landing'ить **только вместе** с serde, иначе dangling-сигнатура. До serde — `http.get(url)?.json()->JsonValue` (динамический).

### 3. effect-free addr + umbrella `real_net()` — SAFE (уже подписано §13.2)

- **Что:** `SocketAddr.loopback(port)` / `@port`/`@ip` — pure `.nv` (retract `AddrNet`); один `with Net = real_net()` вместо 4 отдельных `with`.
- **Ценность:** `bind(loopback(8080))` без `with`; серверный common-path с одним umbrella-`with`.
- **Цена:** rep-change `SocketAddr` (C-handle → value-record `{family,[]u8,port}`), byte-baseline-guarded коммит, **НЕ блокирует HTTP** (`178-std-http.md:927`).
- **Вердикт:** SAFE. Гранулярность держится: `connect`/`bind`/`serve` по-прежнему объявляют `TcpNet`/`DnsNet` в сигнатуре (`:1098`); pure addr-операции честно ретрактируют эффект (считают, а не делают I/O).

### 4. must-consume чист на typed-json-пути — SAFE-WITH-CAVEAT (подтверждение, не новый механизм)

- **Что:** `.json[T]()` уже consume'ит `Body`. На typed-json-пути ручной `@drain` не возникает ни на одной ветке.
- **Разбор веток:** happy → `.json[T]()` consume'ит; `error_for_status` Err → `?`-bubble уносит `HttpResponse` целиком в `Err`-возврат до материализации живого `Body`-binding'а; в `get_json` у пользователя `HttpResponse` вообще нет в руках — ноль discharge-обязательств.
- **Вердикт:** SAFE-WITH-CAVEAT. Load-bearing допущение: «ранний `?`-возврат consume-значения = корректный discharge» (стандартная linear-семантика «moved into return value»). Требует pos+neg-теста (см. ниже), иначе допущение, а не доказанный инвариант.

### 5. явный `with`/builder остаётся для app/test — SAFE (анти-регрессия)

Root-install влияет **только** на free `http.*`. App (`with Http = real_http() { app() }`), test (`with Http = mock_http().on(...)`), tuned client (`HttpClient.builder().timeout().proxy().build()?`) — без изменений 1:1.

## Что СОХРАНИТЬ (не упрощать)

1. **`Http` в сигнатуре** — НЕ убирать из типов (как сделал `Async`/D62). `Async` звучно ambient, потому что он runtime-mechanic, не resource-capability. `Http` — resource-capability (его подменяют handler'ом, это весь смысл testability-win). Убрать из сигнатур = убить инварианты (2) и (4).
2. **mockability через R1-шэдоуинг** — `with Http = mock_http()` обязан побеждать root-дефолт по обычному inner-wins lookup'у. Дефолт — нижний кадр стека, НЕ catch-all в обход lookup'а.
3. **must-consume body** — свойство типов `HttpResponse`/`Body`, не трогается install'ом. neg-тест защищает.
4. **`forbid Http {}` непреодолим** — sentinel `FORBID(Http)` push'ится ВЫШЕ корневого дефолта → lookup видит его первым. Плюс compile-уровень D63 смотрит на сигнатуры, не на стек — не зависит от того, установлен ли дефолт. Дефолт даже **усиливает** требование явности песочницы.

## Честно про trade-off

**Единственный реальный размен — (1) против сегодняшнего «нет `with` → compile-fail».** Сейчас забытый mock в тесте = compile-error (функция несёт `Http`, на корне теста handler'а нет). После default-install забытый `with Http = mock_http()` **тихо подхватит root `real_http` и пойдёт в реальную сеть** — точная копия contextvars-«забыл замокать → утечка».

Закрывается **полностью и нормативно**: дефолт авто-активен **только в script-edition**; lib/app/**test**-edition требуют явный корневой `with` (root `Http` пуст в тестах → забытый mock падает, как сегодня). Это превращает размен из регрессии в edition-гейт. Условие обязано быть нормативным в плане, не опциональным guard'ом. Образец — Go `context.Background()` назван явно (выигрывает у thread-local именно явностью на корне), а не прячется.

Почему остальная explicitness НЕ разменивается: `fn () Http -> ...` в сигнатуре — это осознанный плюс Nova (инвариант 2), и один токен `Http` дешевле, чем `#[tokio::main]`/`async`/`suspend`/`throws` у пиров. Тут Nova не проигрывает — упрощать нечего.

## Что записать в Plan 178

- **§3.0 / §3.5 (client one-shots):** добавить полный verb-набор (`delete`/`put`/`patch`) + `get_json[T]`/`post_json[T,B]`; зафиксировать auto-`error_for_status` в `*_json` с doc-fence (инверсия Q4) + escape-hatch.
- **§12 → §13 (H2):** новый под-пункт «default-install root `real_http`» — закрывает остаток H2 (после §13.2 umbrella). Семантика дефолта = runtime-root handler рядом с fiber-scheduler/root-`Io` (D62), нижний кадр стека.
- **§13 нормативное условие (критично):** root `real_http` инсталлируется **только в script-edition**; lib/app/test-edition — пустой корень `Http`. Без этого (1) деградирует с compile-time-гарантии до runtime-сюрприза.
- **§8 / §8.0 acceptance:** (a) mock-handler-тест MANDATORY для эффект-модуля; (b) pos+neg must-consume-тест на `?`-discharge (pos: `get_json` happy+4xx компилируются без ручного drain; neg: забытый consume `HttpResponse` всё ещё compile-fail); (c) serde-gate: `*_json` landing'ится только вместе с Plan 184.
- **§13.3 (discharge-таксономия):** не меняется — `@drain` остаётся для «не нужно тело», на typed-json-пути не возникает.
- **D327 (effect-контракт):** дополнить упоминанием root-install-семантики `Http` (тот же механизм, что root-`Io`).

## Итог

Common-path: `http.get_json[T](url)` без `with` внутри `main` со script-edition root-`real_http`. Паритет с `requests.get(url).json()` по церемонии, превосходство над reqwest по async (фибры). Все 4 инварианта целы: (1) mock через R1-шэдоуинг (при test-edition-гейте дефолта); (2) `Http` в сигнатуре + D28-lift; (3) Result + json-consume; (4) `forbid Http {}` непреодолим. Единственная нетривиальная стоимость — runtime-bootstrap root-install (механизм root-`Io` уже существует), и единственный размен полностью закрыт edition-гейтом дефолта.