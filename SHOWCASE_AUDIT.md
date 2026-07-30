# Аудит обходов в витрине — SHOWCASE_AUDIT.md

**Дата:** 2026-07-30  
**Ветка:** `showcase-workaround-audit`  
**Модель:** claude-3.5-sonnet-v2 (gpt-4o via opencode)  
**Периметр:** `examples/flagship/aggregator/`, `examples/` (вне `_wip/`), `nova-polaris/examples/`

---

## Таблица находок

| № | файл:строка | форма | канон | причина | жив ли? | класс |
|---|---|---|---|---|---|---|
| 1 | aggregate.nv:79-111 | `fetch_one` возвращает `Result[SourceData, AggError]` вместо `throw`/`Fail[AggError]` | `throw AggError.Upstream(id)` / `Fail[AggError]` effect | `[M-flagship-spawn-throw-segfault]` — SIGSEGV при `throw` multi-field enum из spawn | ✅ **ЗАКРЫТ** (RE-VERIFY — свежий бинарь не воспроизводит; фикстура `regressions/spawn_throw_multifield_payload/`). Комментарий: «менять её обратно на throw не требуется (Result читабельнее)». | НЕ ОБХОД — осознанное решение (Result читабельнее для mixed-исходов). |
| 2 | aggregate.nv:122-148 | `fetch_one` принимает поля `Source` по-отдельности (`id str, latency_ms int, ...`), а не весь `Source` | `fn fetch_one(source Source, idx int)` | `[M-flagship-spawn-capture-value-struct-ptr-mismatch]` — CC-FAIL при захвате value-структуры в два уровня `spawn` | ✅ **ЗАКРЫТ** (RE-VERIFY 2026-07-20; фикстура `regressions/spawn_capture_value_struct/`). Комментарий: «вернуть можно отдельной byte-churn-волной». | НЕ ОБХОД — осознанное решение (скалярная сигнатура не мешает, возврат не приоритетен). |
| 3 | aggregate.nv:176 | Chain разбита на отдельные биндинги | `deadline_elapsed_ms` одной цепочкой | `[M-174.1-vec-method-chain-elem-erasure]` — facade-`[]T` метод теряет тип элемента при чейнинге | 🔴 **ОТКРЫТ** (P3, backlog-followups.md:1901) | А' (в витрине) — ждёт фикса |
| 4 | aggregate.nv:195-198 | Явная аннотация `Monotonic` на каждом `Monotonic.now()` | `ro t0 = Monotonic.now()` (без аннотации) | `[M-flagship-monotonic-now-bare-binding-ice]` — ICE при bare-binding `Monotonic.now()` без аннотации | ✅ **ЗАКРЫТ** (попутно M-176 Path→Member фиксом 67717dcb1/747a79c65; фикстура `regressions/monotonic_now_bare_binding/`) | СНЯТЬ СЕЙЧАС — аннотации больше не нужны, код безвреден но загромождает |
| 5 | aggregate.nv:207-212 | `ro t0 Monotonic = Monotonic.now()` с аннотацией | bare `ro t0 = Monotonic.now()` | `[M-p67-path-call-const-receiver-method-ice]` — const-ресивер + generic-extension метод | ❓ **ЖИВ?** (221.1-bug-sweep.md:303: «живой независимо, если когда-то проявится своим репро») | Прочее — дубль №4 (обе аннотации от одного корня). |
| 6 | aggregate_test.nv:52 | Явный `SourceData { ... }` префикс в enum-variant аргументе | `TaskStatus.Done({ id, payload: ... })` | `[M-flagship-anon-record-literal-enum-payload]` — bare record literal как enum-variant payload путает codegen | 🔴 **ОТКРЫТ** (статус не установлен в реестре; упомянут только локально) | А' (в витрине) — ждёт фикса |
| 7 | aggregate_test.nv:108-132 | Утверждения на подстроку сериализованного JSON вместо round-trip через `Json.parse` | `json_encode(dto) |> json_parse |> ...` | `[M-flagship-nested-hashmap-jsonvalue-mono]` — 3-level nesting CC-FAIL | 🔴 **ОТКРЫТ** (статус не установлен в реестре) | А' (в витрине) — ждёт фикса |
| 8 | aggregate_test.nv:132 | Комментарий: "auto-derive doesn't support field-skip yet, `[M-180-serde-field-attributes]`" | `#serde(skip)` на `Option` поле в DTO | `[M-180-serde-field-attributes]` — field-customization (skip) не был реализован | ✅ **ЗАКРЫТ** для record-типов (Plan 180.1, 2026-07-22, D435). Поле `error` сериализуется как `null` (не пропускается). | СНЯТЬ СЕЙЧАС — комментарий устарел, field-skip уже работает для record-типов. |
| 9 | aggregate_test.nv:205-227 | int-коды событий (1/2/3) вместо `str`-тегов; литералы `1`/`2`/`3` вместо именованных `const` | `Channel[str]` с `str`-тегами + `const EVT_DONE = 1` | `[M-flagship-channel-str-recv-typaret]` + `[M-flagship-handler-lit-const-capture]` | 🔴 **ОТКРЫТЫ** (оба статус не установлен в реестре) | А' (в витрине) — ждёт фикса |
| 10 | live.nv:257-332 | `t0.plus(budget)` вместо `t0 + budget` | `t0 + budget` (операторная форма) | `[M-187-monotonic-plus-operator-mono]` — duplicate operator mono-collision в одном CU | 🔴 **ОТКРЫТ** (статус не установлен в реестре) | А' (в витрине) — ждёт фикса |
| 11 | report_json.nv:100 | Структурный подсчёт `fibers = 2 * results.len()` вместо `fibers.slot_count()` | `fibers.slot_count()` runtime introspection | `[M-187-leaks-introspection]` — `slot_count()` no-op sentinel на Windows | 🔴 **ОТКРЫТ** (платформенное ограничение Windows, а не баг компилятора) | Прочее — платформенное ограничение, структурный подсчёт корректен по построению |
| 12 | main.nv:528 | `EmitRecord` как heap record (не `value`) + `Channel[EmitRecord]` | value-record `type EmitRecord value { ... }` + `Channel[EmitRecord]` | `[M-channel-generic-elem-type]` / `[M-channel-elem-type-not-tracked]` — `Channel` требует word-safe payload; multi-field value-record не word-safe | 🔴 **ОТКРЫТ** (221.1-bug-sweep.md №143, 🔴 P1; отдельный `[M-channel-real-elem-type-inference]` P2) | А' (в витрине) — ждёт фикса |
| 13 | main.nv:269 | `detach` с consume (старый `detach { stream ... }` переписан на `detach consume stream`) | `detach { stream.read(...) }` (без consume) | `[M-detach-consume-escape-unchecked]` — detach не ловил use-after-consume | ✅ **CLOSED** (commit `5065f684d`, 2026-07-22; backlog-followups.md:86) | СНЯТЬ СЕЙЧАС — коммент устарел, код уже на canonical `detach consume stream` |
| 14 | main.nv:317 | `[M-187-supervised-nested-fiber-slot-race]` — комментарий об обновлённом reasoning | — | План 83.4.5.12 | ✅ **ЗАКРЫТ** | СНЯТЬ СЕЙЧАС — коммент исторический, код не содержит обхода |
| 15 | main.nv:94-118 | admission control (bounded-accept `MAX_INFLIGHT_CONNS`) | без лимита | `[M-187-high-concurrency-connection-wedge]` — connection wedge при большом паралеллизме | ⚠ **MITIGATED** (ограничение, не фикс) | НЕ ОБХОД — «admission control stays because honestly any server needs one» (осознанное инженерное решение) |
| 16 | main.nv:255 | `[M-187-sequential-2nd-request-hang]` note внутри loop | — | не установлен | ❓ **Статус не установлен** | НЕ ОБХОД — референс в комментарии, код не меняет |
| 17 | main.nv:146,357 | `[M-187-nested-spawn-scope-var-cc-fail]` — NOT worked around in-place | — | компиляторный баг, не обходится в этом файле | 🔴 **ОТКРЫТ** (статус не установлен в реестре) | НЕ ОБХОД — код не меняет, только контекст |
| 18 | main.nv:475-515 | Обширная историческая справка о resolved дефекте `[M-187-http-serde-setcookie-serialize-collision]` | — | resolved | ✅ **RESOLVED** (2026-07-16; backlog-followups.md:134) | НЕ ОБХОД — историческая справка о закрытом дефекте; код уже на `snapshot_to_json` |
| 19 | main.nv:508-514 | Историческая справка о `[M-json-encode-record-field-order-nondeterministic]` | — | ✅ **ЗАКРЫТ** (2026-07-29; 221.1-bug-sweep.md №148) | ✅ **ЗАКРЫТ** | НЕ ОБХОД — историческая справка; код уже на `json_encode` canonical |
| 20 | domain.nv:123 | Комментарий о `[M-slice-ext-receiver-for-in-elem-type]` как бывшем gate carrier | `for r in @` (работает) | Был: receiver element type inference не работал для `fn []TaskResult @to_report` | ✅ **ЗАКРЫТ** (2026-07-19; backlog-followups.md:187) | СНЯТЬ СЕЙЧАС — коммент устарел, код уже каноничный |
| 21 | report_json_test.nv:8-12 | Комментарий об `[M-187-errorkind-parsejsonerror-variant-collision]` | split в отдельный модуль `domain/` | Вариант collision двух `UnexpectedEof` из разных модулей | ✅ **FIXED** module-boundary fix | НЕ ОБХОД — уже исправлено реальным фиксом, код корректен |
| 22 | net/echo_client.nv:40 | `ro eb []u8 = echo_bytes` (явный тип) перед вызовом `unsafe { eb.to_str_unchecked() }` | `echo_bytes.to_str_unchecked()` напрямую | `[M-174.1-vec-method-chain-elem-erasure]` — chain теряет конкретный элемент | 🔴 **ОТКРЫТ** (P3) | Прочее (пример, не флагман) |
| 23 | tls/echo_client.nv:63 | То же: `ro eb []u8 = echo_bytes` | `echo_bytes.to_str_unchecked()` напрямую | `[M-174.1-vec-method-chain-elem-erasure]` | 🔴 **ОТКРЫТ** (P3) | Прочее (пример, не флагман) |
| 24 | ffi/sqlite_mini.nv:7,36 | `[M-115-ffi-build-pipeline]` — real link integration gated | реальная линковка с libsqlite3 | FFI build pipeline (`--c-shim` CLI) не построен | 🔴 **ОТКРЫТ** (P3; backlog-followups.md:3436) | Прочее (пример sketch, не флагман) |
| 25 | ffi/sqlite_mini.nv:47 | `unsafe { ... }` wrapper вокруг каждой `extern "nova" fn` с raw-указателями | (канон для `unsafe` по D424 rule 1) | `[M-174.6-rawptr-extern-unsafe-infer]` — automatically inferred `unsafe` for raw-ptr extern | ❓ **Статус не установлен** (212-audit-150plus-closeout.md:46: 📋) | Прочее (пример, не флагман) |
| 26 | polaris/03-json-api/src/main.nv:34 | `TypedRoute` прямой struct literal вместо `typed(handler_fn)` | `@post_typed` / `typed(post_handler)` wrapper | `[M-user-generic-value-type-as-struct-field]` — generic value-тип как поле структуры не работает | 🔨 **В РАБОТЕ** (221.1-bug-sweep.md №139, 🔴 P1, раунд 3) | А' (в витрине polaris) — ждёт фикса |
| 27 | polaris/05-auth/src/main.nv:21 | `JwtAuth.claims_at` явный clock (`demo_now` far in future) | `claims_at(now_ms)` with ambient `Time` | `[M-example-fixed-clock]` — фиксация времени для демо, чтобы токен не истекал | НЕ обход | НЕ ОБХОД — осознанное решение для демо-примера |
| 28 | emit.nv:24 | `[M-178-server-streaming]` референс | — | ✅ **ЗАКРЫТ** (force-impl, 2026-07-12) | ✅ **ЗАКРЫТ** | НЕ ОБХОД — историческая справка |
| 29 | main.nv:66 | `[M-187-sse-live-stream]` — scoped-out | — | Требует эффект-несущего пути в соединение | 🔴 **ОТКРЫТ** (scoped-out, не в этом плане) | НЕ ОБХОД — scoped-out feature, не workaround |
| 30 | main.nv:62 | `[M-178-server-streaming]` / `StreamBody.from_chunks` + `ServerResponse.sse` | — | ✅ **ЗАКРЫТ** | ✅ **ЗАКРЫТ** | НЕ ОБХОД — используется каноничный API |

---

## СНЯТЬ СЕЙЧАС

Обходы поверх уже закрытых дефектов — чистая уборка, риска нет:

| № | Место | Что сделать | Причина закрытия |
|---|---|---|---|
| 4 | aggregate.nv:195-198 | Убрать явную аннотацию `Monotonic` с `Monotonic.now()` (строки 200, 208) | `[M-flagship-monotonic-now-bare-binding-ice]` ✅ ЗАКРЫТ (commit 67717dcb1/747a79c65) |
| 8 | aggregate_test.nv:132 | Снять комментарий: "auto-derive doesn't support field-skip yet" — теперь `#serde(skip)` работает | `[M-180-serde-field-attributes]` ✅ CLOSED for record types (Plan 180.1, 2026-07-22) |
| 13 | main.nv:269 | Снять/обновить упоминание `[M-detach-consume-escape-unchecked]` как workaround — код уже на `detach consume stream` | ✅ CLOSED (commit `5065f684d`, 2026-07-22) |
| 14 | main.nv:317-357 | Обновить/снять упоминание `[M-187-supervised-nested-fiber-slot-race]` | ✅ ЗАКРЫТ (Plan 83.4.5.12) |
| 20 | domain.nv:123 | Снять комментарий "gate carrier" — `for r in @` работает с фиксом | ✅ ЗАКРЫТ (2026-07-19) |

---

## ЖДЁТ ФИКСА

Обходы поверх живых дефектов:

| № | Место | Маркер | Что блокирует | Статус в реестре |
|---|---|---|---|---|
| 3 | aggregate.nv:176 | `[M-174.1-vec-method-chain-elem-erasure]` | Chain method return type erasure | 🔴 P3 (backlog-followups.md:1901) |
| 6 | aggregate_test.nv:52 | `[M-flagship-anon-record-literal-enum-payload]` | Bare record literal as enum payload confuses codegen | не найден в реестре |
| 7 | aggregate_test.nv:108-132 | `[M-flagship-nested-hashmap-jsonvalue-mono]` | 3-level nesting in JSON parse CC-FAILs | не найден в реестре |
| 9 | aggregate_test.nv:205-227 | `[M-flagship-channel-str-recv-typaret]` + `[M-flagship-handler-lit-const-capture]` | Channel[str] recv miscompiles; handler-lit const capture CC-FAILs | не найдены в реестре |
| 10 | live.nv:257-332 | `[M-187-monotonic-plus-operator-mono]` | Duplicate `+` operator mono-collision in one CU | не найден в реестре |
| 12 | main.nv:528 | `[M-channel-real-elem-type-inference]` (бывш. `[M-channel-generic-elem-type]`) | Channel только word-safe payloads | 🔴 P1 (221.1-bug-sweep.md №143) |
| 22-23 | echo_client.nv:40, tls/echo_client.nv:63 | `[M-174.1-vec-method-chain-elem-erasure]` | Chain return type erasure | 🔴 P3 |
| 24 | ffi/sqlite_mini.nv:7,36 | `[M-115-ffi-build-pipeline]` | FFI `--c-shim` CLI не реализован | 🔴 P3 (known_red) |
| 26 | polaris/03-json-api/main.nv:34 | `[M-user-generic-value-type-as-struct-field]` | Generic value-type as struct field CC-FAILs | 🔨 В РАБОТЕ (221.1-bug-sweep.md №139) |

---

## НЕ ОБХОД

Что могло бы выглядеть как обход, но является осознанным решением:

| № | Место | Обоснование |
|---|---|---|
| 1 | aggregate.nv:79-111 | `Result` вместо `throw` — осознанно, читабельнее для mixed-исходов; баг закрыт |
| 2 | aggregate.nv:122-148 | Скалярные параметры вместо `Source` struct — осознанно, возврат не приоритетен; баг закрыт |
| 11 | report_json.nv:100 | Структурный подсчёт вместо runtime introspection — окна нет, платформенное ограничение |
| 15 | main.nv:94-118 | Admission control — любой сервер его имеет, не работаунд |
| 16-17 | main.nv:146,255,357 | Референсы на баги — код не меняют, только контекст |
| 18-19 | main.nv:475-515 | Исторические справки о закрытых дефектах — код уже каноничный |
| 21 | report_json_test.nv:8-12 | Историческая справка о закрытом дефекте |
| 27 | polaris/05-auth/main.nv:21 | Фиксация времени для демо — осознанное решение |
| 28-30 | emit.nv:24, main.nv:62,66 | Референсы на закрытые/scoped-out маркеры |

---

## Проверка двух известных мест

### `emit_record_json` (реестр №141)

**Статус: ✅ ЧИСТО.** `EmitRecord` теперь `#impl(Serialize)` + `json_encode` (main.nv:550-565). Старая ручная склейка УБРАНА ПОЛНОСТЬЮ. Комментарии (main.nv:475-515) описывают историю — что было, почему revert, как переоткрыли после №148. Это историческая справка, не объявление текущего обхода.

### `[M-json-encode-record-field-order-nondeterministic]` (реестр №148)

**Статус: ✅ ЧИСТО.** Упоминание на main.nv:508 явно говорит "fixed the root cause... №148 closed". Не представлено как «текущее ограничение». Требование выполнено.

---

## Итоговые числа

| Метрика | Значение |
|---|---|
| Всего находок (референсы включая НЕ ОБХОД) | 30 |
| Обходов в витрине (класс А') | 8 (№3,6,7,9,10,12,26 + polaris №26) |
| Из них в `examples/flagship/aggregator/` | 7 (А') |
| СНЯТЬ СЕЙЧАС (чистая уборка) | 5 (№4,8,13,14,20) |
| ЖДЁТ ФИКСА | 8 (№3,6,7,9,10,12,22-23,24,26) |
| НЕ ОБХОД (осознанное) | 15 |
| Статус не установлен | 2 (№25, №16-p-187-sequential-2nd-request-hang) |

**Резюме:** Флагман агрегатор содержит 7 активных обходов класса А' (блокеры тега). 5 мест можно безопасно вычистить уже сейчас — это комментарии и аннотации, оставшиеся от закрытых дефектов. 8 обходов ждут компиляторных фиксов. Наибольший блокер: `[M-channel-real-elem-type-inference]` (№12) и `[M-174.1-vec-method-chain-elem-erasure]` (№3) — оба в коде, не только в комментариях.
