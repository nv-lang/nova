<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# План 209 — Ф.0 recon: дизайн multi-TU codegen + карта Ф.1/Ф.2

**Модель:** opus. **База:** worktree `nova-209` @ `71f732f3e` (branch `plan209-recon`).
Только чтение исходников; код не менялся, гейт не запускался, в main не писалось.

Все якоря — `compiler-codegen/src/codegen/emit_c.rs` (55 784 строки) если не сказано иное.

---

## 0. Как СЕЙЧАС собирается финальный `.c` (карта эмиссии)

Точка входа: **`emit_module(mut self, module) -> Result<(String, Vec<String>)>`** — `emit_c.rs:4197`.
Возвращает финальную C-строку + warnings в `emit_c.rs:7489` (`Ok((self.out, warnings))`).

Весь вывод копится в **ОДИН** `String` — поле `self.out`. `self.line(s)` = `push_str(s)+"\n"`.
Ряд под-секций копится в отдельные буферы-строки (`mono_fwd_decls`, `deferred_impls`,
`novaopt_typedefs_buf`, `novares_typedefs_buf`, `value_record_defs_buf`, `lambda_impls`, …)
и в конце **splice'ится текстовой заменой** в маркеры-плейсхолдеры внутри `self.out`.

### Порядок сборки `self.out` (сверху вниз)

1. **`emit_preamble()`** — `emit_c.rs:7836` (вызов `:5164`). Эмитит первые строки + серию
   маркеров-плейсхолдеров (сам вывод пуст, заполняется в finalize):
   - `:7847` `/*__EFFECT_COUNT_MARKER__*/` — **ОБЯЗАН быть строкой 1** (build-слой читает
     `nova-effect-count: N` и раздаёт `-DNOVA_MAX_EFFECT_STORAGES=N` во ВСЕ TU).
   - `:7849` `#include "nova_rt/nova_rt.h"` (runtime include).
   - Маркеры (порядок важен для C-зависимостей typedef'ов):
     `:7858 __LEGACY_TUPLE_TYPEDEFS__`, `:7865 __USER_TYPE_FWD_DECLS__`,
     `:7869 __TYPEID_DEFINES__`, `:7873 __PER_E_FAIL_DECLS__`,
     `:7878 __VALUE_RECORD_DEFS__`, `:7891 __MONO_TUPLE_TYPEDEFS__`,
     `:7896 __EXTERN_FN_PROTOS__`, `:7902 __MONO_FIXARR_TYPEDEFS__`,
     `:7907 __NOVAOPT_TYPEDEFS__`, `:7911 __NOVARES_TYPEDEFS__`,
     `:7919 __BUILTIN_SUM_METHOD_FWD_DECLS__`, `:7924 __INTERNED_STR_LITERALS__`.
2. **User type fwd-decls / type defs** — цикл `:5185+` (`emit_type_decl`).
3. **Free-fn / method forward decls** — цикл `:6676-6683` (`emit_fn_forward_decl`, `:13596`).
   Затем маркеры `:6685 __MONO_FWD_DECLS__`, `:6700 __STRUCT_EQ_PROTOS__`,
   `:6701 __NOVAOPT_EQ_FNS__`, `:6709` fwd-decls тестов, `:6717` handler fwd-decls.
4. **Определения функций** — цикл `:6724-6731` (`emit_fn`, `:23568`) → append в `self.out`.
5. `:6734` embed-proxy тела; `:6737-6745` тела тестов; `:6756-6813` bench-тела;
   `:6815-6930` **drain mono-worklist** (`emit_monomorphized_fn`/`_method` append в out);
   `:6932 deferred_impls` (handler-тела); `:6944 render_consts_init_fn` (`nova_consts_init`);
   `:6947 emit_main_wrapper`.
6. **Finalize — splice-замены** маркеров (`self.out.replace(...)`), `:6959-7465`:
   `USER_TYPE_FWD_DECLS :6959`, `VALUE_RECORD_DEFS :6968`, `NOVAOPT_TYPEDEFS :6991`,
   `NOVARES_TYPEDEFS :7002`, `BUILTIN_SUM_METHOD_FWD_DECLS :7018`, `GENERIC_TYPE_DEFS :7023`,
   `NOVAOPT_VR_TYPEDEFS :7035`, `NOVARES_VR_TYPEDEFS :7047/7053`, `MONO_FWD_DECLS :7057`,
   `VR_UEQ_PROTOS :7063`, `STRUCT_EQ_PROTOS :7070/7076`, `NOVAOPT_EQ_FNS :7080/7086`,
   `MONO_TUPLE_TYPEDEFS :7180`, `EXTERN_FN_PROTOS :7186/7194`, `MONO_FIXARR_TYPEDEFS :7251`,
   `LEGACY_TUPLE_TYPEDEFS :7273`, `SUPERVISOR_DECIDE_IMPL/INIT :7357/7358`,
   `TYPEID_DEFINES :7412`, `SCOPE_TIMEOUT_IMPL/INIT :7448/7449`, `PER_E_FAIL_DECLS :7453`,
   `INTERNED_STR_LITERALS :7462`. Затем strict-error gate `:7476`, return `:7489`.

**Ключевой вывод.** Верхняя зона (preamble + fwd-decls, до цикла тел `:6724`) — *в основном*
декларации, НО в неё через маркеры **вклеиваются и ОПРЕДЕЛЕНИЯ** (interned-строки в preamble,
per-E TLS-состояние, тела eq-fn в `__NOVAOPT_EQ_FNS__`). Значит «резать по строке начала тел»
недостаточно — нужна классификация деклараций vs определений (см. §5).

---

## 1. Тулчейн: компиляция + линковка (объём Ф.2)

`compiler-codegen/src/test_runner.rs`. Сборка команды: **`build_command(tc, opts)`** — `:1303`;
универсальная точка `compile_c_to_exe` — `:1961`.

**Сейчас — ОДИН вызов компилятора** (clang/cl/gcc) на всё: сгенерённый `.c` + ~15 runtime-`.c`
передаются как исходники в одну команду, компилятор внутри собирает и **линкует в exe**:
- `:1622` `c.arg("-o").arg(opts.exe_file)`; `:1623` `c.arg(opts.c_file)` (сгенерённый);
- `:1624-1633` runtime: `rt_alloc, rt_effects, rt_fibers, rt_fiber_arena[_win], rt_fiber_stats,
  rt_runtime, rt_driver, rt_typeid, rt_segv_diag` (список формируется `:1305-1341`;
  условно `net.c`, `fs.c`, `brotli_shim.c`).
- Второй бранч (MSVC/иначе) — `:1912` тот же `-o exe`.

**Разделения на `.o` + отдельный линк СЕЙЧАС НЕТ** — один драйвер-вызов `.c…→exe`. Компилятор
обрабатывает TU **последовательно** (даже если дать несколько `.c` одной командой — драйвер
не параллелит). ⇒ чтобы получить wall-clock выигрыш, Ф.2 обязана: (1) компилировать каждый
`part_i.c → part_i.o` **параллельно** своим пулом потоков; (2) слинковать `.o` → exe.
Runtime-`.c` можно (нужно) компилировать в `.o` один раз и кешировать (они не меняются между CU).

Env для effect-count: `-DNOVA_MAX_EFFECT_STORAGES=N` раздаётся во ВСЕ TU (`:7837-7847` doc,
build читает первую строку `.c`). В multi-TU N кладём в `common.h` строкой 1 (её читает build),
каждый `part_i.c` инклудит `common.h`, флаг раздаётся всем частям + всем runtime-`.o` одинаково.

---

## 2. Развилка 1 (ГЛАВНАЯ) — `static` через границу TU

### Факт: ВСЁ пользовательское — `static`
- Free-fn / метод forward-decl: `:14001` `self.line("static {} {}({});")`; sret-вариант `:14010`.
- Тела и прочие top-level символы эмитятся `static` повсеместно (греп `"static "`): методы
  (`:13808, :13122`), mono-инстансы, синтез-thunk'и (`:8695`), lambda/blk/spawn/drain
  (`:11340, :12223, :12508, :12835, :12979`), typeinfo (`:7407`), interned-строки
  (`:7956, :7971`), lazy-const хранилище (`:8027`), `nova_consts_init` (`:8126`), тесты/бенчи
  (`:7804, :7563`), supervisor/timeout (`:7308, :7437`).
- Nova **не экспортирует C-ABI** (FFI только Nova→C через `is_external`); в исполняемом бинаре
  ВСЕ символы внутренние, линкуются лишь внутри одного финального exe (док `:301-306`).

### Уникальность имён (мангл) — CU-wide уже гарантирована
- File-private free-fn: `nova_fn_<mod>_f<file_id>_<name>` — `:4753-4788` (дискриминатор = стабильный
  `file_id` пира ⇒ одноимённые file-private в разных файлах → РАЗНЫЕ C-символы).
- File-private const: `Nova_const_<mod>_f<file_id>_<name>` — `:4790-4818`.
- D381 collision-aware мангл (`colliding_type_names`/`colliding_fn_names`, `:696-715`,
  `current_emit_file_id` в `emit_fn_forward_decl :13605`) разводит кросс-модульные коллизии.
- Синтез-символы: монотонный счётчик (`_nova_blkfn_N`, `_nova_spawn_N`, …) или хеш контента
  (interned `nova_blob_<hash>`) ⇒ CU-уникальны.

### РЕКОМЕНДАЦИЯ: вариант (а) — ПРОМОУШН в external. Граф-разбиение НЕ нужно.
Обоснование: имена **уже** CU-уникальны (см. выше) ⇒ снятие `static` со всех top-level
fn/глобалов не даёт коллизий линковки; поскольку линк в один exe, external-linkage безопасен.
Граф взаимозависимых static'ов (вариант б) не требуется — любую функцию можно положить в любой
part, лишь бы её **прототип** был виден (кладём ВСЕ прототипы в `common.h`, см. §5).

**Что промоутим (снимаем `static`, добавляем `extern`-декл в common.h):**
1. Все top-level **определения функций** (free/method/mono/lambda/thunk/test/bench/eq/supervisor/
   timeout) — `static T f(...)` → `T f(...)`; их прототипы в common.h — `T f(...);` (без `static`).
2. Все top-level **глобалы-объекты**:
   - lazy-const хранилище `_nova_const_X_value` (`:8027`) — **мутабельно, кросс-TU** ⇒ ОБЯЗАН
     быть одно определение + `extern` в common.h; `nova_consts_init` (одна фн) — в один part.
   - per-E Fail TLS `static __thread _nova_handler_Fail_m` (`:2239`) — **мутабельное TLS-состояние,
     наблюдаемое кросс-TU** (installer и throw в разных part'ах обязаны видеть ОДИН слот) ⇒
     одно определение + `extern __thread` в common.h. Throw-`static inline` (`:2247`) можно
     оставить `static inline` в common.h (per-TU дубль inline безопасен, но обращается к общему
     extern-слоту).
   - interned-строки/blob (`static const nova_str`/`uint8_t[]`, `:7956/7971`) и typeinfo
     (`static const NovaTypeInfo`, `:7407`) — read-only; безопасны и как per-TU дубль, но чище
     единым определением + `extern const` в common.h (см. §5, единый инвариант).

**НЕ трогаем `static inline` в заголовочной зоне** (`nova_typeid_user_name :7385`, throw-inline):
inline-функция в common.h, инклуднутая каждым TU, корректна как per-TU internal — снятие `static`
дало бы multiple-definition. Правило хелпера (§ниже) должно различать `static inline` (оставить)
и top-level `static <def>` (промоутить).

**Механизм промоушна (дёшево, за флагом):** единый хелпер `top_level_storage()` возвращает
`"static "` (флаг off, дефолт байт-идентичен) или `""` (multi-TU on). Провести ~40 сайтов эмиссии
top-level `static ` через него. НЕ маршрутизировать `static inline`/локальные `static` внутри тел.

---

## 3. Развилка 2 — стратегия разбиения (детерминизм)

**Текущий порядок функций детерминирован:** тела идут в порядке `module.items` (исходный порядок,
`:6724`), затем drain mono-worklist в порядке обнаружения (`:6844` `std::mem::take` батчами —
FIFO по регистрации), затем deferred_impls, consts_init, main. Один и тот же вход ⇒ один и тот же
порядок и границы.

**РЕКОМЕНДАЦИЯ:** резать по **порогу суммарного размера байт на part** (target ~500 КБ/part),
каждая функция ЦЕЛИКОМ в одном part, обход определений в существующем стабильном порядке:
набираем текущий part пока `bytes < 500К`, на границе функции переносим в следующий. Порог по
байтам (а не по числу функций) точнее бьёт по суперлинейности C-компилятора (она от размера TU).
Число part'ов N = ceil(total_defs_bytes / 500К). Границы = только на границе top-level определения
(никогда внутри тела). Детерминизм: тот же вход → тот же порядок → те же границы → те же part'ы
(нужно для build-cache и воспроизводимости). `common.h` фиксирован (не зависит от разбиения).

---

## 4. Развилка 3 — `common.h`: что точно туда

**В `common.h` (ТОЛЬКО декларации, инклудится каждым part):**
- строка 1: `/* nova-effect-count: N */` (build читает отсюда) + `#include "nova_rt/nova_rt.h"`.
- Все **typedef'ы**: `__LEGACY_TUPLE_TYPEDEFS__`, `__USER_TYPE_FWD_DECLS__`, `__VALUE_RECORD_DEFS__`,
  `__MONO_TUPLE_TYPEDEFS__`, `__MONO_FIXARR_TYPEDEFS__`, `__NOVAOPT_TYPEDEFS__`,
  `__NOVARES_TYPEDEFS__`, `__NOVAOPT_VR_TYPEDEFS__`, `__NOVARES_VR_TYPEDEFS__`,
  `__GENERIC_TYPE_DEFS__`.
- Все **`#define`**: `__TYPEID_DEFINES__` (+ его `static inline nova_typeid_user_name` — оставить
  inline).
- Все **прототипы функций** (промоутнутые, БЕЗ `static`): регулярные fwd-decls (`:14001`),
  `__MONO_FWD_DECLS__`, `__BUILTIN_SUM_METHOD_FWD_DECLS__`, `__EXTERN_FN_PROTOS__`,
  `__STRUCT_EQ_PROTOS__`, `__VR_UEQ_PROTOS__`, fwd-decls тестов/бенчей/handler'ов, lambda-fwd.
- **`extern`-декларации всех глобалов-объектов**: `extern <T> _nova_const_X_value;`,
  `extern __thread ... _nova_handler_Fail_m;`, `extern const nova_str ...;`,
  `extern const NovaTypeInfo NOVA_TYPEINFO_...;`.
- **guard** `#pragma once` / `#ifndef` обёртка.

**Определения (НЕ в common.h) — ровно ОДИН part каждое:**
`__NOVAOPT_EQ_FNS__` (тела eq-fn), `__INTERNED_STR_LITERALS__` (данные), `__PER_E_FAIL_DECLS__`
(TLS-слоты — определения), `__SUPERVISOR_DECIDE_IMPL/INIT__`, `__SCOPE_TIMEOUT_IMPL/INIT__`,
все тела функций, `nova_consts_init`, lazy-const хранилища.

**Глобал с ОПРЕДЕЛЕНИЕМ (не extern):** единый инвариант — **определение в один part + `extern` в
common.h**. Особенно ОБЯЗАТЕЛЬНО для мутабельного кросс-TU состояния (lazy-const хранилища, per-E
TLS) — иначе (а) per-TU дубли → multiple-definition при линке, либо (б) расщеплённое состояние →
разошедшийся runtime. Read-only const (interned/typeinfo) технически можно дублировать, но кладём
по тому же инварианту (единое определение) — проще и без раздувания.

---

## 5. Механизм расщепления Ф.1 — РЕКОМЕНДАЦИЯ (низкий риск, дефолт байт-идентичен)

Два кандидата:
- **(A) Дуал-буфер на этапе эмиссии** (`header_out` vs `body_out`, маршрутизация `self.line`) —
  инвазивно, риск задеть дефолтный путь, много сайтов.
- **(B) Пост-финализация: структурный сплиттер финальной строки** — РЕКОМЕНДУЕТСЯ.

**(B):** дефолтный путь производит ту же одну строку что и сегодня (0 изменений байт).
При multi-TU (флаг + порог) — постпроход `split_tu(&finalized) -> (common_h, Vec<part_c>)`:
1. `top_level_storage()`-хелпер уже снял `static` со всех top-level определений (единственное
   изменение горячего пути, за флагом) ⇒ все top-level символы external + CU-уникальны.
2. Сегментатор проходит финальную строку на **глубине скобок 0**, выделяя top-level единицы
   (декларация → до `;`; определение → до парной `}`), с учётом строк/символьных литералов/
   комментариев. Машинно-сгенерённый top-level регулярен; известные макро-строки
   (`NOVA_BENCH_STATE_DEFINE;`, `NOVA_BENCH_HEAP_SAMPLER_THREAD_DEFINE`, effect-count comment,
   `#include`, `#define`) обрабатываются таблицей.
3. Классификация каждого сегмента: `#include`/`#define`/typedef/прототип(`);` без тела)/
   `extern` → **common.h**; определение функции/глобал-с-инициализатором → **part** (round-robin
   по порогу §3). Для глобала-с-инициализатором дополнительно эмитим `extern`-строку в common.h.
4. Собрать `common.h` (+guard, +effect-count строкой 1) и `part_0..K.c`
   (`#include "<cu>_common.h"` + сегменты).

Плюс (B): расщепление — чистая трансформация вывода, включается только выше порога; байт-паритет
РЕЗУЛЬТАТА (вывод программы) сохраняется, т.к. семантика C идентична (те же символы, тот же линк).

---

## 6. Развилка 4 — порог включения

Multi-TU включать только если размер зоны определений (или всего `.c`) **> 2 МБ** ИЛИ
**> ~200 функций** (что раньше). Ниже — оставляем один `.c` (сплит+линк-оверхед не окупается).
Порог замерить на реальном распределении (conformance 13 МБ/963 файла — далеко выше; мелкие
std-CU — ниже). Флаг: `NOVA_MULTI_TU=1`/CLI `--multi-tu[=auto]`; `auto` = по порогу.

## 7. Развилка 5 — build-cache-ключ

`nova-cli/src/build_cache.rs`: ключ = `DefaultHasher` над версией-строкой `"nova-c-cache-v2"`
(`:85`), байтами исходников (`:117-121`), features/target_os/mono_depth/contracts_mode/
strict_effects (`:101-111`), embed-файлами (`:128-132`). Кешируется ОДИН `.c` (`:139`
`<key>.c`). **Multi-TU меняет форму артефакта** (common.h + N .c) ⇒:
- добавить в хеш: флаг multi-TU (on/off), порог байт, версию стратегии сплита;
- **бампнуть версию-строку** `"nova-c-cache-v2"` → `"v3"` (инвалидация старого кеша);
- кешировать **набор** (common.h + part'ы) — напр. директория `<key>/` с манифестом, либо
  `<key>.tar`; ключ = хеш(входы ⊕ split-params). Смена порога/стратегии → новый ключ.

---

## 8. КАРТА РЕАЛИЗАЦИИ (атомы, порядок, модель, риск)

Байт-паритет **РЕЗУЛЬТАТА** (вывод программ), НЕ раскладки `.c`. Проверка: тот же набор
фикстур прогнать в single-TU и multi-TU режимах, сравнить stdout/exit (не `.c`). Плюс инвариант
уникальности символов.

### Ф.1 — codegen split (за флагом `NOVA_MULTI_TU`, дефолт = старый путь)
- **A1 (sonnet).** Хелпер `top_level_storage(&self) -> &'static str` (`"static "`|`""` по флагу).
  Провести ~40 сайтов top-level `static ` (список в §2) через него. НЕ трогать `static inline`
  header-хелперы и локальные `static` в телах. Риск: **средний** — пропустить сайт → останется
  `static` определение, невидимое кросс-part (undefined-ref при линке). Митигация: грепом
  собрать полный список сайтов до правки; ассерт-инвариант A3.
- **A2 (sonnet).** Сегментатор `split_tu(finalized:&str, threshold:usize) -> (String, Vec<String>)`:
  скан глубины-0 с учётом `""`/`''`/`//`/`/* */`; таблица известных макро-строк; классификатор
  декларация/определение; сборка common.h (+`#pragma once`, +effect-count строка 1,
  +`extern`-декларации глобалов) и part'ов (`#include common.h`). Риск: **высокий** — единственная
  нетривиальная логика; покрыть unit-тестами на синтетических TU (typedef, прототип, тело, глобал
  с инициализатором, bench-макро).
- **A3 (haiku).** Debug-ассерт: собрать имена всех top-level определений в multi-TU, проверить
  **уникальность** (дубль = сбой промоушна/мангла); проверить, что каждый вызываемый символ имеет
  прототип в common.h. Риск: низкий; это страховка ГЛАВНОГО риска.
- **A4 (sonnet).** Прошить возврат `emit_module`: при флаге отдавать `(common_h, Vec<part>)`
  (нов. поле результата / обёртка), при выкл — как сейчас `(String, _)`. Порог-гейт §6 здесь.
  Риск: средний (сигнатура наблюдается несколькими вызывающими — `main.rs`, `test_runner.rs`,
  `bench/run.rs`; см. §0). Держать back-compat обёртку.

### Ф.2 — тулчейн (параллель + линк)
- **B1 (sonnet).** В `test_runner.rs`: записать `common.h` + `part_i.c` на диск рядом
  (`compile_c_to_exe`/`build_command`, `:1303/:1961`). Риск: низкий.
- **B2 (sonnet).** Компиляция `part_i.c → part_i.o` **параллельно** пулом потоков
  (число = `num_cpus`), с теми же флагами что сейчас у одиночного `.c` (include-пути, `-D`
  effect-count, GC/FFI/brotli — §1). Риск: средний (флаги должны совпасть для ВСЕХ part'ов;
  вынести формирование флагов в общий билдер).
- **B3 (sonnet).** Runtime-`.c` (`:1305-1341`) компилировать в `.o` один раз + кешировать
  (не меняются между CU). Риск: низкий.
- **B4 (sonnet).** Линк `part_*.o` + runtime `.o` + libs → exe (clang: `clang -o exe *.o -l…`;
  MSVC: `link.exe`/`cl` link-фаза). Порог-гейт §6: ниже порога — старый одиночный путь. Риск:
  средний (линк-строка libs/GC/brotli как в `:1568-1640`).
- **B5 (haiku).** build-cache §7: бампнуть версию-строку, добавить split-params в хеш, кешировать
  набор (common.h+part'ы). Риск: низкий.

### Ф.3 (вне рекона) — включить для conformance, замер до/после, гейт байт-паритета результата.

---

## 9. Главный РИСК и НЕОПРЕДЕЛЁННОСТИ

**ГЛАВНЫЙ РИСК.** Промоушн `static`→external на символе, чьё имя НЕ уникально CU-wide (пропущенный
кейс мангла) → тихая коллизия линковки / not-uniquely-defined. Митигация: ассерт-инвариант
уникальности имён (A3) + предварительный греп всех top-level `static`-сайтов (A1). Вероятность
низкая (мангл уже уникален — §2), но цена высокая.

**Вторичный риск.** Сегментатор (A2) неверно классифицирует редкий top-level конструкт (макро-
строка, глобал-с-инициализатором со скобками, много-строчный инициализатор) → декларация уедет в
part или определение в common.h → multiple-definition / undefined-ref. Митигация: unit-тесты +
таблица известных макросов; fallback — при неуверенности классификации отключить multi-TU для CU.

**НЕОПРЕДЕЛЁННОСТИ (решить в Ф.1/Ф.2):**
1. Точное множество top-level `static`-сайтов (грепнуть исчерпывающе перед A1 — здесь оценено ~40).
2. per-E Fail TLS и lazy-const хранилища — подтвердить, что все обращения идут через имя (не через
   локальную реэмиссию), чтобы `extern`+единое определение было достаточно (ожидается да).
3. MSVC-путь линка отдельных `.o` (`link.exe` vs `cl` двухфазно) — синтаксис флагов иной, чем clang
   (`:1912`-бранч), нужен отдельный билдер линк-команды.
4. Порог 2 МБ / 500 КБ-на-part — калибровать замером на реальных CU (Ф.3).
5. Идентичность `-DNOVA_MAX_EFFECT_STORAGES=N` и прочих `-D` во ВСЕХ part'ах И runtime-`.o`
   (ABI-инвариант effect-registry) — вынести в единый набор флагов (B2).
