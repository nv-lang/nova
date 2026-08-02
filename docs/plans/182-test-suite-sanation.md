<!-- SPDX-License-Identifier: CC-BY-4.0 -->
# Plan 182 — Санация тестового корпуса `nova_tests/` (черновик)

> **Уровень:** Top-level (инфраструктура тестов).
> **Создан:** 2026-07-06 (аудит выполнен 2026-07-06/07). **Статус:** `📋 ✅ ЗАКРЫТ 2026-07-09 (санация выполнена волнами 2026-07-06..08; довливные классы раннера — маркерами в backlog) (аудит-карта + план фаз; правок в nova_tests НЕ вносилось)`.
> **Преемник:** [Plan 198](198-nova-tests-triage.md) (spec_tests-миграция корпуса) продолжает трек.
> **Маркер:** `[M-182-test-sanation]`.
> **Запуск:** «выполни план 182».
> **Мотив (владелец, 2026-07-06):** «`nova_tests` наполовину сломан, хотели удалить; много CU
> не по конвенциям [test-conventions.md](../dev/test-conventions.md), много дубликатов». Источник истины
> о звучности — `spec_tests/conformance` (язык+прелюдия) + `std/<модуль>/*_test.nv` (std-модули) —
> конвенция ред. 2026-07-06; `nova_tests/` — НЕ гейт корректности
> ([feedback-nova-tests-not-correctness-gate]), только baseline-DELTA, и **ЗАМОРОЖЕН** для новых
> тестов. Цель трека — послойно перенести ценное и удалить корпус.
> **Этот заход — ТОЛЬКО аудит + план (read-only по nova_tests; единственное добавление —
> транзитная парковка d358, см. §4).**

---

## 0. Методология аудита (честно про инструмент)

- **Прогон:** `nova test --full <dirs>` (C-codegen pipeline; HEAD-current бинарь `nova`,
  сборка 2026-07-06 10:11 — ПОСЛЕ мерджа 172.5/172.12 и серии codegen-фиксов 07:21–09:08).
  Раннер репортит per-CU (folder-module = один entry; каждый `neg/`-файл = свой entry).
- **⚠ Стейл-бинарь даёт ложные RED.** Прогон бинарём 07:17 (до 172.5) давал массовые false-RED:
  весь `inout_ref/` (2 pos + 11 neg, фикстуры Plan 172.5) «падал» на парсинге `mut ref`; на
  HEAD-бинаре — зелёный. Вывод: **классификация валидна только на свежем бинаре** (Ф.0).
- **⚠ P67-LEGACY internal-error АВАРИЙНО роняет весь раннер** (Rust-panic, exit 101): один битый CU
  обрывает прогон целиком. Полный `nova test nova_tests` СЕГОДНЯ НЕ ДОХОДИТ ДО КОНЦА — крашеры
  выявлены бисекцией/пер-каталожным прогоном и исключены `--skip`. **17 каталогов-крашеров** (§3.3) —
  главная механическая причина «наполовину сломан»: любой полный прогон умирает на первом же.
- **⚠ Параллельность искажает результаты:** при `--jobs 24` наблюдались load-таймауты здоровых
  тестов (67с у `expected_runtime/stdout_hello`, `generics/mono_basic`) и lld-link «cannot open
  output file» (зомби-процессы TIMEOUT-тестов держат exe). Такие каталоги перепрогнаны соло;
  TIMEOUT-строки из параллельного прогона помечены как подозрительные (§3.5).
- **Дубли:** md5 всех 3493 `.nv` → **0 байт-идентичных групп**; дублирование — топическое (§4).

## 1. Инвентарь

| Метрика | Значение |
|---|---|
| Top-level папок | **257** (207 `planNN*`, 50 тематических) |
| `.nv` файлов | **3493** |
| CU-entry (раннер `--full --list`) | **1484** |
| — negative (`/neg/`) | **795** |
| — positive-ish (folder-module/panic/exit/stdout) | **689** |
| `neg/`-поддиректорий | 122 |
| вложенных `nova.toml`-пакетов | 30 (в осн. `plan03_1/*`) |
| `*_slow.nv` | 59 |
| Маркеры | COMPILE_ERROR 749 · RUNTIME_PANIC 126 · TIMEOUT 78 · STDOUT 63 · STDERR 22 · EXIT 6 |

## 2. Классификация папок (HEAD-бинарь; агрегация ~1406 per-CU строк)

| Класс | Папок | Доля | Комментарий |
|---|---|---|---|
| **GREEN** (все CU зелёные/SKIP) | **144** | 56% | список — Приложение А |
| **RED** (≥1 падающий CU) | **93** | 36% | ~210 падающих CU; таксономия §3.1-3.2 |
| **CRASH** (P67 internal-error, роняет раннер) | **17** | 7% | §3.3; их CU не полностью замерены |
| **EMPTY** (нет тест-CU: lib-only) | **3** | 1% | `plan40_sanitizers`, `plan57_e2e`, `plan70_3` |

Статусы CU (классифицированные ~1406 строк): PASS ≈ 1181 · CODEGEN-FAIL 84 · TIMEOUT 50 ·
CC-FAIL 34 · NEG-NO-ERROR 25 · SKIP 15 (z3-гейт) · NEG-WRONG-MSG 14 · NEG-WRONG-PANIC 3.
Остаток до 1484 — внутри 17 CRASH-каталогов (прогон обрывается до конца каталога).

**Худшие RED-каталоги (fail/total):** `plan103_8` 14/14 CODEGEN-FAIL · `plan67` 14/14 TIMEOUT ·
`plan91_8c` 13/14 CODEGEN-FAIL · `plan03_1` 10/11 · `plan124` 10/36 (неги) · `plan159` 8/19 ·
`plan115` 6/11 TIMEOUT · `plan144_0` 6/10 TIMEOUT · `plan83_stress_armed` 5/5 · `expected_runtime`
5/23. Хвост: ~60 каталогов с 1-4 падениями (полный RED-список — Приложение Б).

## 3. Таксономия причин

### 3.1 RED — устаревший API / изменившийся контракт (чинить/удалять ТЕСТ)
| Причина | Примеры CU |
|---|---|
| `str.len()` ретирован (`E_STR_NO_LEN`) | `concurrency/_repro_p110`, `contracts/apply_arity_suggest_w2402`, `gc/bounded_rate` |
| `Mul on option` (снятая семантика) | `effects/basic` |
| pkg-dep негативы: `E_D78_MODULE_PATH_MISMATCH` маскирует целевую диагностику | весь `plan03_1/*` (10 CU) |
| NEG-NO-ERROR (правило ослаблено/снято) | `generics/neg/plan101_1_*` (prefix-shadow, 3), `plan108/neg/*E_LOCAL_NOT_MUT*` (4), `plan118/neg/*write*ro*` (2), `plan168/neg/*vec_*_type_mismatch` (2), `plan124/neg/*` (часть из 10), `plan62/neg` (3), `plan153_0/neg` |
| NEG-WRONG-PANIC (panic-message контракт сменился) | `expected_runtime/defer_panic_mainflow`, `defer_throw_single`, `multi_expect_stdout` |
| D133-not-consumed (стар. must-consume API) | `http_typed/typed_json_test` |
| Extension-метод: видимость через import ужесточена (`E_EXTENSION_METHOD_NEEDS_IMPORT`/`E7320`) | `plan153_5_nested/control_flat` |

### 3.2 RED — компилятор-БАГИ, вскрытые фикстурами (чинить КОМПИЛЯТОР, тест сохранить)
- **CC-FAIL, невалидный C:** `basics/control_flow` (undeclared ident), `modules/priority_queue`
  (assignment to cast), `named_params/imported_named_run` (init type mismatch), `plan159/f2_*+f3_*`
  (5 CU: `nova_str`↔`int` init mismatch), `plan106/guard_ok_ro`, `plan107/no_prelude_attr`,
  `plan144_inftype` (2), `plan91_fe4`, `unicode` (1), `plan103_4/barrier_all_or_none_prop`.
- **CODEGEN-FAIL:** `plan103_8` (14 CU litmus/prop/stress — весь каталог),
  `plan91_8c`+`plan91_8c_direct` (17 CU), `plan83_stress_armed`/`83_11`/`83_4_5_6_stress` (12 CU),
  `plan103_6/blocking_atomic_fetch_add_ok`, `plan108_4/pos_consume_close_match`,
  `plan153_3_1/neg_sort_wrong_cmp_type` (method-level typearg inference `K` для
  `sort_unstable_by_key`), `plan162_2` (2, «failed to write .c» — проверить соло).

### 3.3 CRASH — P67-LEGACY internal-error: 17 каталогов (подтверждены на HEAD-бинаре)
| Каталог | Симптом (`emit_c.rs`) |
|---|---|
| `map_literals` | `.insert_new` method-call return unknown (:41704) |
| `plan48_mpm` | `.produce` method-call return unknown (:42333) |
| `plan57` | `.opaque` method-call return unknown (:42333) |
| `plan60` | `.capacity` method-call return unknown (:42333) |
| `plan65` | Path-call return unknown `Time.after` (:42544) |
| `plan83_12` | Path-call return unknown `bind` (:42544) |
| `plan154_1` | Path-call return unknown `from_debug` (:42544) |
| `plan114` | Ident `make_adder__closure_2` not in var_types (closure-from-const-fn, :42632) |
| `plan143` | Ident `leaf_double` (forwarder / preempt_elision, :42632) |
| `plan143_2` | Ident `callback_target` (:42632) |
| `plan34`, `plan56`, `plan83_6`, `plan83_10` | Ident `gc` not in var_types (:42632) |
| `plan70_1` | Ident `h` (:42632) |
| `plan99` | Ident `y` (:42763) |
| `plan153_4` | Index element type unknown (`chunks_windows`, :40123) |

Сюда же — **cross-module sum-имя-коллизия**, обнаруженная при разнесении std-тестов (Часть 1
2026-07-06): whole-module test-CU `std/http` крашится, потому что `ErrorKind` объявлен и в
`std.http`, и в `encoding.compress`, а name-keyed `sum_schema_registry` берёт чужую схему →
pattern-binding compress-only вариантов (`InvalidData(msg)`/`BadHeader(msg)`) не регистрируется →
panic в `emit_match`. Библиотечный (import) режим не триггерит — DCE не эмитит недостижимое.

Всё семейство — name-keyed-registry / P67-канал ([M-172.1-var-types-cu-name-leak]).
→ **Тесты КОРРЕКТНЫ (гонят реальные баги) — НЕ удалять; чинить компилятор.** До фикса — крашеры
исключить из дефолт-регресса: они роняют ВЕСЬ прогон и маскируют статус остальных 240 каталогов.

#### 3.3-РЕЗОЛЮЦИЯ (Plan 182 CRASH-трек, 2026-07-07, ветка `crashers-182`)

Прогон 17-ти на СВЕЖЕМ бинаре (после мерджей 176-184): **8 из 17 уже НЕ крашат** (аудит был
на стейл-бинаре) — `plan48_mpm`/`plan65`/`plan154_1`/`plan34`/`plan56`/`plan83_6`/`plan83_10`/
`plan99` дают PASS либо честно-RED (assert/neg-drift), НЕ P67 (раннер не рушат). Оставшиеся 9 —
кластеризованы по корню, 4 корня закрыты, 3 задокументированы:

- **[ЗАКРЫТ] cross-module ErrorKind-коллизия (http-блокер + семейство).** Корень НЕ в
  sum_schema_registry, а в `should_skip_type` (emit_c.rs, `emit_module` §D29-shadow): name-keyed
  skip дропал НЕ-entry-модульный `ErrorKind` (компресс), т.к. simple-name совпадает с http-овским
  → его sum-схема НЕ регистрировалась → `Ident 'msg' not in var_types`. Fix: исключить
  `colliding_type_names` из shadow-skip (коллизирующие типы уже квалифицированы в РАЗНЫЕ C-базы
  `Nova_std_http_ErrorKind`/`Nova_encoding_compress_ErrorKind` — emit обоих redefinition-safe).
  C-доказательство: `variant_sum_candidates(InvalidData)=[]`, `Other=[Method, std_http_ErrorKind]`
  (компресс отсутствовал). Тесты `body/model/url`-парковки перенесены в `std/http/*_test.nv`;
  `d358` слит в `model_test` (subset). Коммит `b3d49c3a9`.
- **[ЗАКРЫТ] first-class-fn-value** (`plan143`/`plan143_2`/`plan114`): `ro g = leaf_double` —
  Ident на free-fn в value-position. Emit уже лоуэрит через `emit_free_fn_value`→`void*`, а
  infer-арм не имел зеркала → ICE. Fix: `user_fn_sigs.contains(name) → "void*"`. Коммит `4bee03689`.
- **[ЗАКРЫТ] module-alias member-call** (`plan70_1`): `import … as h` + `h.add_one(41)` →
  `infer(Ident("h"))` ICE. Fix: `imported_modules.contains(name) → ""` (namespace-sentinel,
  last-resort) → method-call путь резолвит через `fn_ret_<method>`. Коммит `6adae28c1`.
- **[ЧАСТИЧНО ЗАКРЫТ — 172.12 A5 заход-9, 2026-07-07] method-call return unknown**
  (`map_literals` `.insert_new`, `plan60` `.capacity`, `plan57` `bench.opaque`): **insert_new+capacity
  УЖЕ НЕ крашат** (porting/крашер-волна в main до A5). **bench.opaque ЗАКРЫТ** (коммит `b9b8008f4`):
  `emit_expr` лоуэрит intrinsic в `NOVA_BENCH_OPAQUE_PRIM` (black-box identity), зеркалю в `infer_expr_c_type`
  (return = тип аргумента) в ОБЕ триплет-копии (43737+47456); bench-тест P67→PASS, conformance 66/0 δ0.
  Остаётся `examples/net` `.close` (erased-receiver ветвь, ниже). [M-182-crash-method-ret-unknown].
- **[ОСТАЁТСЯ — checker-гэп] Path-call return unknown** (`plan83_12` `bind`): `Path`-форма вызова
  `TcpListener.bind` без `fn_ret`/registry-записи. **Probe (A5 заход-9): chan=None, fnret/mo/external ВСЕ пусты —
  у кодогена НЕТ источника, хотя `nova check` PASS** (чекер знает `Result[TcpListener, NetError]`, но не ПЛЮМИТ
  в `resolved_types[call.id]`). ⇒ fix = **checker-аннотация** (types/mod.rs), НЕ emit_c-fallback.
  [M-182-crash-pathcall-ret-unknown], emit_c.rs:~44011.
- **[ОСТАЁТСЯ, корень найден] nested-generic collect** (`plan153_4` `chunks_windows`):
  `v.chunks(2).collect()` даёт `[][]int`, но return-инференс `BoxIter[Vec[T]].collect()`
  эрейзится в `nova_int` → `cs[0]` Index на nova_int → panic. Корень — mono-return nested-generic
  collect, НЕ Index-сайт. [M-182-crash-nested-generic-collect-erase], emit_c.rs:41498.

Доп-уловы из задания (тот же класс): `examples/net` — теперь `.close` return unknown,
obj_ty=`nova_int` (ресивер `lst` эрейзнут в nova_int) → тот же [M-182-crash-method-ret-unknown]
(erased-receiver ветвь). `std/time/timer_metrics_test` — **CC-FAIL** (не P67-краш, честный RED):
`NovaValue_Timestamp` инициализируется `int` (value-record init mismatch, §3.2) —
[M-182-crash-value-record-init-int]. `[M-plan62-hashable-flap-runtime]` — недетерминированный
RUN-FAIL (не воспроизведён в этом заходе; рантайм/GC-гонка кодогена hashable, оставлен как есть).

Гейты фиксов: сборка обоих крейтов зелёная; conformance `--full` **66/0** (дельта 0); prelude-
shadow (`plan72/p3a_record_shadow_range_pos`, `plan62/range_shadow_warning`, `plan138_2/t16`,
`plan107/allow_shadow_attr`) зелёные — collision-exempt не ломает D29-shadow; broad-sample
(closures/sums/modules/generics) — 0 новых крашей.

### 3.4 SKIP (не RED) — гейт окружения
z3-gated contracts-тесты (`NOVA_SMT_BACKEND=z3`) — 13-15 SKIP без z3. Легитимно.

### 3.5 FLAKY / TIMEOUT-подозрительные
- Каталоги с плотным `EXPECT_TIMEOUT`: `plan103_3/4/6/8`, `plan83_11/12`, `concurrency`, `sync`.
- **`plan67` — 14/14 TIMEOUT в СОЛО-прогоне** → похоже на реальные хэнги (fiber-джойны?), не load.
- TIMEOUT-строки `plan123*`-семейства (12 каталогов по 1 CU), `plan115` (6), `plan144_0` (6),
  `expected_runtime` (2) получены в прогоне `--jobs 24` — **вероятные load-артефакты**;
  в Ф.0 перепроверить соло прежде чем записывать в RED.

## 4. Дубликаты / overlap

- **std-модульные папки `nova_tests/` vs конвенция `std/<модуль>/*_test.nv`** (ред. 2026-07-06):
  `http` (+ `d358_http_message_model` — транзитная парковка из Части 1 до фикса ErrorKind-коллизии;
  негативы `neg/d359_*` — near-dup с `std/http/neg/d359_*`), `io` (`d322_*`), `fs` (`d323_*`-неги
  near-dup c `std/fs/neg/`), `os`, `compress`, `serde`, `serde_e2e`, `time`, `unicode`,
  `str`/`strings`.
- **D-номерные фикстуры в nova_tests:** 25 файлов `dNNN_*` (d322/d323/d326/d359/d65/d72/d96/d196 …)
  — сверить пофайлово с `spec_tests/conformance` и `std/**/*_test.nv`; near-dup удалить.
- **plan-vs-тема:** `plan123/plan123_2_licm_for_loop_ok` ≈ `plan123_2/licm_for_loop_ok` и подобные —
  консолидировать по темам (test-conventions §Консолидация, Plan 169.1.2).

## 5. Ценность

- **Уникальные регресс-репро (СОХРАНИТЬ):** 17 CRASH-каталогов (§3.3) и CC-FAIL/CODEGEN-репро
  (§3.2) — живые баг-репро компилятора; `_repro_*`, `p176repro`, `cgfix_*`.
- **Свежие зелёные модульные тесты треков 176-181 (мигрировать, Ф.2):** `io`, `fs`, `os`,
  `compress`, `serde`, `serde_e2e`, `time`, `http`/`http_decompress`/`http_server`/
  `http_transport`/`http_typed` (все GREEN), `plan176_holes`, `plan178`, `plan180_f1` — написаны
  в этой сессии по старой конвенции, подлежат переносу к модулям.
- **Покрыто conformance (кандидаты на удаление после пофайловой сверки):** языковые темы с
  dNNN-аналогом в `spec_tests/conformance` (as-cast, coalesce, defer, consume, value-record, …).

## 6. Фазы санации

**Ф.0 — Ре-бейзлайн на свежем бинаре + крашер-скиплист.** Пересобрать `nova`; прогнать корпус с
`--skip` всех 17 крашеров §3.3; TIMEOUT-подозрительные (§3.5) — соло; зафиксировать
воспроизводимую карту. Без этого классификация недостоверна (стейл-бинарь = false-RED).

**Ф.1 — Изолировать крашеры + завести компилятор-баги.** 17 каталогов §3.3 → скиплист
дефолт-регресса; по классам симптомов — `[M-…]`-маркеры в P67/172.1-трек (method-call return,
Path-call return, closure/forwarder Ident, Index element, sum-schema коллизия). Тесты не удалять.

**Ф.2 — Миграция свежих зелёных модульных тестов сессии 176-181 → рядом с модулями.**
По конвенции 2026-07-06 (прецедент — Часть 1 этого захода): `nova_tests/{io,fs,os,compress,serde,
serde_e2e,time}` + `http*`-семейство + `plan176_holes`, `plan178`, `plan180_f1` →
`std/<модуль>/<имя>_test.nv` (позитив, module-декларатор модуля, без self-import) /
`std/<модуль>/neg/` (негатив, standalone `module neg.<имя>`). `d358` (транзит в `nova_tests/http`)
→ `std/http` ПОСЛЕ фикса ErrorKind-коллизии (§3.3). Near-dup неги (d359/d323) — слить.
Критерий: число проверяемых норм до/после совпадает; `nova test std` зелёный.

**Ф.3 — Починить/удалить RED-по-контракту (§3.1).** Каждый CU: обновить под текущую норму ЛИБО
удалить как покрытый (`spec_tests`/`std`-тестами). `plan03_1` — переспецифицировать под D78/D29
rev-3 (10 CU, вся суита пакетных зависимостей).

**Ф.4 — Компилятор-трек по §3.2** (nova_str/int-init `plan159`, кодоген `plan103_8`/`plan91_8c`/
`plan83_*`, `sort_unstable_by_key` K-inference); фикстуры остаются регрессом этих фиксов.

**Ф.5 — Консолидация тем + флаки-ревью.** plan-vs-тема дубли (§4) слить в тематические
folder-module; `plan67` и TIMEOUT-плотные (§3.5) — детерминизм-ревью по test-conventions §Флаки.

**Ф.6 — Удаление остатка.** После миграции ценного и заведения баг-маркеров — удалить
покрытое/устаревшее; корпус сокращается монотонно до нуля.

## 7. Критерии

- Каждая фаза: ничего ценного не потеряно (миграция ≥ удаление по числу проверяемых норм);
  `spec_tests` + `std`-тесты всё-зелёные; дефолт-регресс не крашится; `nova_tests` монотонно
  сокращается.

---

## Приложение А — GREEN-каталоги (144)

`any_is atomics buffers cfg cgfix_fluent_tail_if compress effect_registry err173
err173_0 err177_collectors ffi fs http http_decompress http_server http_transport http_typed
inout_ref io narrowing negative_capability os p176repro plan100_1 plan100_2 plan100_3 plan100_4_1
plan100_4_2 plan100_4_4 plan100_4_5 plan100_6 plan100_7 plan101_1 plan103_5 plan103_9
plan104_7_grammar plan104_9 plan110 plan110_9_np plan120 plan123_1 plan123_1_1 plan123_2
plan123_2_1 plan123_3 plan123_3_2 plan123_4 plan123_4_2 plan123_4_4 plan125_followups plan126
plan126_2 plan127 plan127_1 plan128_2 plan132 plan133 plan134 plan136 plan136_1 plan138 plan138_1
plan138_2 plan138_3 plan138_5 plan140_2 plan140_3 plan140_4 plan142 plan145 plan148 plan149
plan149_toml plan150 plan153_1 plan153_2 plan153_2_zc plan153_3 plan153_5 plan153_6 plan154
plan160 plan161 plan162 plan162_1 plan163 plan164 plan167 plan169 plan169_2_blanket plan170
plan172 plan172_boxiter_width plan172_composition plan172_neg_member_not_concrete
plan172_neg_mixed_signedness plan172_neg_multiple_sets plan172_neg_not_in_set plan172_stdlib_neg
plan176_holes plan178 plan180_f1 plan36_d1 plan48_1 plan59 plan61 plan63 plan70 plan76 plan77
plan79 plan82_2 plan83_10_3 plan83_10_4 plan84 plan87 plan88 plan89 plan90 plan90_1 plan91_10
plan91_14 plan91_15 plan91_16 plan91_7 plan91_8a_2 plan91_8b plan91_fe2 plan91_fe5 plan95
plan95bis plan96 plan96_1 plan97 plan98 plan_parser_recsep plan_value_iter rebind serde serde_e2e
str syntax_probe time vec_elem_type`

## Приложение Б — RED-каталоги (93, fail/total [виды])

```
14/14 plan103_8 [CODEGEN]      14/14 plan67 [TIMEOUT-solo]   13/14 plan91_8c [CODEGEN]
10/11 plan03_1 [CG,NWM]        10/36 plan124 [NNE,NWM]        8/19 plan159 [CC,CG,TO]
 6/11 plan115 [TIMEOUT†]        6/10 plan144_0 [TIMEOUT†]     5/23 expected_runtime [NWP,TO†]
 5/5  plan83_stress_armed [CG]  4/19 generics [NNE,TO†]       4/21 plan108 [NNE]
 4/4  plan83_11 [CG]            4/5  plan91_8c_direct [CG]    3/62 plan118 [CG,NNE]
 3/4  plan156 [CC]              3/14 plan62 [NNE]             3/3  plan83_4_5_6_stress [CG]
 2/6  plan107 [CC,CG]           2/3  plan118_1_unsafe_extern_fn [CG,NWM]
 2/4  plan11_followup [CC,TO†]  2/6  plan128 [CG]             2/11 plan144_inftype [CC]
 2/3  plan162_2 [CG]            2/3  plan168 [NNE]            2/3  plan55 [CG,NNE]
 2/27 plan91_12 [CC,TO†]        2/2  plan91_fe4 [CC,CG]       2/39 strings [CG,NWM]
 2/17 sync [CC,TO†]             1/1  basics [CC]              1/3  concurrency [CG]
 1/83 contracts [CG]            1/11 doc [CG]                 1/3  effects [CG]
 1/1  gc [CG]                   1/1  http_servernet [TO†]     1/31 modules [CC]
 1/8  named_params [CC]         1/5  plan100_8 [TO†]          1/1  plan103_3 [TO†]
 1/14 plan103_6 [CG]            1/16 plan103_4 [CC]           1/13 plan106 [CC]
 1/7  plan108_4 [CG]            1/3  plan118_1_7 [CG]         1/2  plan118_1_cstr_nul [TO†]
 1/1  plan123 [TO†]  (+11 каталогов plan123_* по 1/1 TO†)     1/5  plan125 [CC]
 1/4  plan125_1 [CC]            1/10 plan125_2 [CG]           1/2  plan135 [CG]
 1/10 plan140 [CG]              1/13 plan140_1 [CC]           1/1  plan141 [CG]
 1/6  plan144_1 [CC]            1/2  plan144_checker [NWM]    1/1  plan145_2 [CC]
 1/38 plan147 [CC]              1/1  plan153_0 [NNE]          1/1  plan153_3_1 [CG]
 1/2  plan153_5_nested [CG]     1/1  plan172_showcase [CC]    1/1  plan172_stdlib_use [CC]
 1/1  plan70_2 [CC]             1/5  plan72 [NNE]             1/12 plan73 [CG]
 1/1  plan74 [CC]               1/10 plan81 [CC]              1/1  plan83_7 [CC]
 1/1  plan91 [CG]               1/1  plan91_13 [CG]           1/1  plan91_8a [CG]
 1/3  plan91_fe1 [CG]           1/26 protocols [CG]           1/1  runtime [CG]
 1/3  self_nested [CG]          1/1  syntax [CG]              1/1  types [CG]
 1/12 unicode [CC]
```
Виды: CG=CODEGEN-FAIL, CC=CC-FAIL, NNE=NEG-NO-ERROR, NWM=NEG-WRONG-MSG, NWP=NEG-WRONG-PANIC,
TO=TIMEOUT; **†** — TIMEOUT из параллельного прогона `--jobs 24` (вероятный load-артефакт,
перепроверить соло в Ф.0).
