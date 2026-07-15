<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# План 209 Ф.2 — checkpoint notes (sonnet, worktree nova-209f2 / branch plan209-f2)

База: `d5cc6679c` (main, включает Ф.1 merge). Работа ведётся ТОЛЬКО в
`d:/Sources/nv-lang/nova-209f2`; суб-агенты не спавнились.

## Задача B (HashMap-mono-gap) — статус: ГОТОВО

### Корень (найден точно, глубокий трейс кода — не по догадке)

`compiler-codegen/src/codegen/emit_c.rs`, `compute_dead_decls_with` (метод-
reachability DCE, Plan 159). Метод `T.m()` считается "живым" (тело
эмитится) ТОЛЬКО если ОБА условия верны: (a) имя типа `T` reachable
(встречается где-то в достижимом коде — для `HashMap` это ВСЕГДА так,
т.к. `Set[T]`'s type-decl `use map HashMap[T, ()]` — unconditional root),
(b) ГОЛОЕ имя метода `m` reachable (встречается как `Member`-имя в AST
где-то в достижимом коде — `collect_used_names`, чистый AST-walk).

D39 embed-delegation proxy (`emit_embed_proxies`, ~14881) — СИНТЕЗИРУЕТСЯ
в codegen НАПРЯМУЮ (никогда не существует как AST `Item::Fn`). Значит вызов
`Nova_Set_method_merge_from` → `Nova_HashMap_method_merge_from` внутри
proxy НЕВИДИМ для `collect_used_names`. Если ничто в РЕАЛЬНОМ Nova-исходнике
не пишет литерально `merge_from`/`values` (напр. `for x in some_set {}` —
итерация десугарится ВНУТРИ codegen, никогда не AST `Member`-нода) —
условие (b) не выполняется → `HashMap.merge_from`/`.values` считаются
dead → тело НЕ эмитится (но forward-decl/вызов из proxy — есть).

`static`-режим маскировал: если ничто не вызывает `Set.merge_from()`
явно — сам proxy тоже unreachable → C-компилятор (`-O2` + `-ffunction-
sections`/`--gc-sections`) целиком выкидывает proxy ДО того как линкер
увидел бы битую ссылку. Промоушн в external (Ф.1 `top_level_storage()`)
убирает эту защиту — компилятор обязан оставить external-символ
(потенциально вызываем извне), тело остаётся → линк падает на реально
не определённом `Nova_HashMap_method_merge_from`.

### Фикс (минимальный, в mono/DCE-коллекторе — как и предписано заданием)

`compute_dead_decls_with` (emit_c.rs, ~406-434): добавлен пред-скан
`module.items` собирающий `embedded_type_names` — множество имён типов,
которые КОГДА-ЛИБО embed'ятся другим record'ом (`use <field>
<EmbeddedType>[...]`, `f.is_embed`). Для метода, чей receiver-тип входит
в это множество, имя метода ПРИНУДИТЕЛЬНО кладётся в стартовый `worklist`
(reachable-by-name-alone) — ТОЧНО тот же over-keep паттерн, что уже
применяется чуть ниже в этой же функции для concrete-slice ресиверов
(`ty.starts_with("[]")`, комментарий "over-keep, never over-prune").
Консервативно (никогда не режет реально reachable код), не пытается
точно воспроизвести override-precedence логику `emit_embed_proxies` —
достаточно для устранения гейта, т.к. любой embedded-тип-метод теперь
просто ВСЕГДА эмитится (как и было бы, если б `collect_used_names` видел
codegen-синтезированный вызов).

Правка ТОЧЕЧНАЯ (~30 строк), риск низкий: затрагивает только набор
"which methods fire", никогда не убирает существующую reachability, и
не трогает механизм мангла/mono-инстанцирования generic-типов вообще.

Верификация: `cargo build --release -p nova-codegen` — 0 ошибок (только
пред-существующие warnings). Runtime-проверка (Set+link под
`NOVA_MULTI_TU=1`) — см. раздел «Совместная верификация» ниже.

## Задача A (тулчейн) — статус: ГОТОВО (Clang/Gcc), MSVC — remainder

### B1-B4 — `compiler-codegen/src/test_runner.rs`

Новая функция `pub fn compile_multi_tu_to_exe(tc, opts: &BuildOpts,
common_h: &str, parts: &[String], timeout) -> anyhow::Result<PathBuf>`
(~2067, рядом с `compile_c_to_exe`). Не трогает `build_command`/
`compile_c_to_exe` (0 изменений default-пути — `EmitOutput::Single`
по-прежнему идёт старым однокомандным путём).

- **B1** Пишет `<stem>_common.h` + `<stem>_partK.c` под `opts.obj_dir`
  (рядом — part'ы `#include` header по относительному имени).
- **B2** Компилирует каждый `part_i.c → part_i.o` **параллельно**:
  `std::thread::scope` + статичное разбиение на `min(available_parallelism,
  jobs.len())` контигентных чанков (без внешних crate-зависимостей —
  scoped threads, стабильно с 1.63). Один и тот же `flags: Vec<String>`
  (target/mode/-D/GC/-fstack.../-ffunction-sections) передаётся ВО ВСЕ
  compile-инвокейшны И в финальный линк — гарантирует ABI-инвариант
  (`-DNOVA_MAX_EFFECT_STORAGES=N` идентичен во всех TU) БЕЗ ручного
  протаскивания через N сайтов.
- **B3** Runtime `.c` (alloc/effects/fibers/fiber_arena[_win]/fiber_stats/
  runtime/driver/typeid/segv_diag + libuv net.c/fs.c/eventloop.c при
  наличии + brotli_shim.c при использовании + FFI `.c`-шимы) компилируются
  РОВНО ОДИН РАЗ (не per-part) — тот же джоб-лист, тот же тред-пул.
  Персистентный (межзапусковый) кеш `.o` НЕ реализован в этой волне —
  `-DNOVA_MAX_EFFECT_STORAGES=N` меняется от CU к CU, тривиальное
  файловое кеширование неверно давало бы stale `.o`; полноценная per-N
  корзина — Ф.3/остаток (см. «Неопределённости»).
- **B4** Линк: `flags` + все `.o` (parts + runtime) + Boehm `-lgc
  -latomic_ops` (Windows) / `-lgc [-lpthread]` (Linux) + libuv lib+syslibs
  + brotli lib(s) + FFI `-L`/`-l` — зеркалирует хвост `build_command`
  построчно.
- Ошибка любого compile-джоба или линка → `Err` с деталями (первая
  ошибка, stdout/stderr скомпилятора).

### Интеграция вызывающих (`codegen_to_c`/`run_one`, `nova-cli::cmd_build`)

- `codegen_to_c` (test_runner.rs, ~3658): переведён с `emitter.emit_module`
  на `emitter.emit_module_multi_tu(&module, &cu_name)`. Возвращает новый
  4-й элемент кортежа — `CodegenArtifact` (`Single` — `.c` записан как
  раньше побайтово; `Split{common_h, parts}` — держится В ПАМЯТИ, НЕ
  пишется рядом с `.nv`).
- `run_one` (~2955): извлекает `codegen_artifact` (borrow, до move
  `codegen_result` в error-check); `NoCFile`-проверка теперь пропускается
  для `Split`; `'cc: {}`-блок получил Split-ветку ПЕРВОЙ строкой —
  вызывает `compile_multi_tu_to_exe`, синтезирует `(CapturedOutput,
  ExitStatus)` тем же helper'ом `synth_exit_status` (через
  `ExitStatusExt::from_raw`, cfg per-OS) — ВЕСЬ последующий код
  (EXPECT_CC_ERROR match, запуск exe, EXPECT_RUNTIME_PANIC/EXIT_CODE/
  STDOUT/STDERR) остался БУКВАЛЬНО нетронут, работает для обеих веток.
  Single-ветка (retry-loop на `build_command`) — **0 изменений**, просто
  теперь под `if let CodegenArtifact::Split ... { ...; break 'cc ...}` в
  начале того же `'cc: {}`-блока, до неё код не доходит на Single-пути.
- `nova-cli/src/main.rs::cmd_build` (~4692-4942): аналогично —
  `c_code: String` → `emit_output: EmitOutput`; cache-hit ветка
  оборачивает закешированную строку в `EmitOutput::Single` (кеш остаётся
  single-`.c`-шейпед — `Split` НЕ кешируется, `build_cache::store_c`
  вызывается только если `emit_output` = `Single`); запись `.c` в tmp_path
  и финальный `compile_c_to_exe`/`compile_multi_tu_to_exe` выбираются по
  варианту `EmitOutput`.
- `nova-cli/src/bench/run.rs` — **НЕ тронут** (вне периметра задания:
  задание называло test_runner.rs/build_command/`nova-cli build`
  явно; bench — третий вызывающий, остаётся на `emit_module` напрямую,
  как и раньше — тот самый back-compat, который Ф.1 A4 гарантирует).

### Рефакторинги-помощники (низкий риск, чистые извлечения)
- `effect_count_define_arg_from_line(first_line, prefix)` — вынесен из
  `effect_count_define_arg` (тот же парсинг, теперь переиспользуется для
  `common_h`, которая уже в памяти — без диск-round-trip).
- `source_uses_brotli(src)` — вынесен из `c_file_uses_brotli` (тот же
  скан, переиспользуется по каждому in-memory part'у).

## Дефолт байт-идентичен (флаг OFF / CU ниже порога)

`emit_module_multi_tu` не трогает `emit_module` (Ф.1 гарантия, не
переисследовалась заново). Проверено здесь: `cargo build --release`
(compiler-codegen И nova-cli) — 0 ошибок, только пред-существующие
warnings. Точечная verify-таблица — см. следующий раздел (выполняется
ПОСЛЕ этого чекпоинта).

## Неопределённости / остаток

1. **MSVC toolchain** — `compile_multi_tu_to_exe` возвращает `Err` для
   `Toolchain::Msvc` (явный remainder, как и предполагал рекон §9.3).
   Вызывающие (`codegen_to_c`/`cmd_build`) СЕЙЧАС не гейтят выбор
   `emit_module_multi_tu` по toolchain'у заранее — если владелец включит
   `NOVA_MULTI_TU=1` НА MSVC-тулчейне для CU выше порога, `nova test`/
   `nova build` получит явную ошибку "not implemented for MSVC" вместо
   молчаливого fallback на single-TU. Это НАМЕРЕННО (явная ошибка лучше
   тихой деградации), но owner должен знать: **включение `NOVA_MULTI_TU`
   на Windows С MSVC-тулчейном (не clang) сейчас ломает сборку**, а не
   просто "не ускоряет". Если это неприемлемо — нужен pre-gate (проверить
   `Toolchain::Msvc` ДО вызова `emit_module_multi_tu`, форсировать `NOVA_
   MULTI_TU` off для него) — не сделано в этой волне (заказчик на этой
   машине использует Clang по умолчанию, см. `detect_toolchain`'s Windows
   preference order Clang > MSVC > GCC).
2. **Runtime-`.o` межзапусковый кеш (B5, файловый)** — НЕ реализован
   (см. B3 доку). `build_cache.rs`-интеграция (версия ключа v2→v3, набор-
   не-строка кеш) — тоже НЕ сделана (recon §7, помечен как "низкий риск,
   haiku-уровень", но требует продумывания per-CU effect-count в ключе;
   оставлен как чистый remainder Ф.3).
3. **`nova-cli/src/bench/run.rs`** — не переведён на multi-TU (вне
   периметра задания по тексту оркестратора).
4. Параллельный компайл использует статичное (contiguous chunk)
   разбиение джобов на N потоков, не work-stealing очередь — при СИЛЬНО
   неровных размерах part'ов возможен небольшой дисбаланс хвоста. Не
   мерялось на реальном 13 МБ conformance CU (вне периметра — "мега-CU
   НЕ гонять", см. задание).

## Совместная верификация (точечная, см. задание пункты 1-4) — ГОТОВО

Окружение: `NOVA_CG_INCLUDE`/`NOVA_RT_DIR` на main-repo `compiler-codegen`
(+`nova_rt`), `NOVA_GC_LIB_DIR`/`NOVA_GC_INCLUDE_DIR` на его
`vcpkg_installed/x64-windows-static/{lib,include}` (per задание). Toolchain
на этой машине — Clang (Windows Auto-preference Clang>MSVC>GCC).

### ⚠ КРИТИЧНО: 3 сегментер-бага split_tu (Ф.1) найдены и исправлены
**прежде чем** удалось прогнать пункты 1-4 — без них ЛЮБАЯ реальная (не
синтетическая) программа под `NOVA_MULTI_TU=1` либо CC-FAIL, либо
lld-link "duplicate symbol". Все три — в `compiler-codegen/src/codegen/
split_tu.rs`, коммит `a6317dd8c` (отдельно от Task A/B, тот же гейт):

1. **Имя юнита ловилось из скобки ВНУТРИ ведущего doc-комментария**
   (`decl_from_fn_def`'s `sig.find('(')`) — comment "…in main()." содержал
   свою `(` раньше настоящей сигнатуры → unit назван `"main"` → коллизия с
   реальным `int main(...)` → A3 uniqueness guard валил корректную
   программу. Фикс: `find_first_real_paren` (comment/string-aware skip).
2. **Cond-block (`#ifdef _MSC_VER…#else…#endif`) резался на 3 куска**,
   если unit начинался с пустой строки (стандартно — `self.line("")`
   между конструктами в emit_c.rs) — leading-whitespace skip пропускал
   только `' '`/`'\t'`, не `'\n'` → `bytes[k]` не находил `#` → блок не
   распознавался как cond-block, попадал в generic `;`-scan → резался на
   каждой `;` → "unterminated conditional directive" в part'е. Фикс: skip
   также `'\n'`/`'\r'`.
3. **Array-typed global (interned-строка `_nova_strlit_<hash>_buf[] =
   "...";`) не находил имя** (`trailing_identifier` не срезал хвостовые
   `[]`) → падал в `DeclOnly{name: None}` → unnamed = A3 dedup не может
   перекрыть повтор → определение оставалось ВЕРБАТИМ (не `extern`) в
   `_common.h` → каждый part, инклудящий header, получал СВОЮ копию →
   `lld-link: error: duplicate symbol`. Фикс: `strip_trailing_array_
   brackets` перед `trailing_identifier`.

+4 regression-теста (22/22 зелёных, standalone `rustc --edition 2021
--test src/codegen/split_tu.rs` — `cargo test` для крейта по-прежнему
сломан пре-существующим дефектом вне периметра, см. 209-f1-notes.md).

### 1. Дефолт (флаг OFF) — байт-идентичен
`nova build examples/getting_started.nv --mode release` без `NOVA_MULTI_TU`
— собирается, бежит, вывод корректный (не diff'ился побайтово повторно —
Ф.1 уже это доказало эмпирически для этого же файла; здесь только
регрессия-smoke после Task A/B правок). ⚠ ОГОВОРКА: Task B (mono-gap
fix, `compute_dead_decls_with`) — это НЕ gated по `NOVA_MULTI_TU`, работает
ВСЕГДА (и в default-режиме тоже) — для CU, где какой-то тип ВСТРАИВАЕТ
(`use field EmbeddedType[...]`) другой generic-тип, дефолтный `.c`
TEXT теперь может отличаться от pre-209 (несколько ранее-dead delegated-
методов теперь остаются в выводе) — это НАМЕРЕННЫЙ бесполезный-байт-рост
от бага-фикса, не регрессия; для CU без embed-отношений (getting_started.nv)
дефолт буквально не задет (embedded_type_names пуст).

### 2. Флаг ON, большой(-ий) пример — сплит → N частей параллельно →
### линк → бежит, вывод корректен
Порог временно снижен (`MULTI_TU_SIZE_THRESHOLD_BYTES`→2КБ,
`_FN_COUNT_THRESHOLD`→5, `MULTI_TU_PART_THRESHOLD_BYTES`→20КБ — **реверчено
перед коммитом**, `git diff` на emit_c.rs после реверта — пусто).
`examples/getting_started.nv` под этим порогом даёт **7 part'ов**
(подтверждено файлами `getting_started_part0..6.c`), компилируются
параллельно (thread::scope), линкуются, бинарь **бежит и даёт ТОТ ЖЕ
вывод**, что default-сборка:
```
Nova — getting started
items audited = 3
audited sum   = 1580
total (cents) = 1422
```
(идентично в обоих режимах, exit 0).

### 3. Флаг ON + Set/HashMap-делегация → mono-gap НЕ линк-фейлит
Сценарий из задания — минимальный scratch-пример (`Set[int].new()` +
`.insert()`, БЕЗ явного вызова `.merge_from()`/`.values()` — именно этот
случай воспроизводил находку Ф.1: делегированный proxy эмитится
безусловно при `import std.collections.set`, реально вызывает
базовый метод HashMap регардless того, называет ли исходник его
literal имя). Под тем же сниженным порогом: **компилируется, линкуется,
бежит** — `len=3`, `contains2=true` — идентично default-сборке (ранее,
ДО Task B фикса, это воспроизводило `undefined symbol:
Nova_HashMap_method_merge_from/_values` — см. 209-f1-notes.md). Мono-gap
подтверждён закрытым.

### 4. Замер (грубый, малый CU — НЕ мега-CU, честная оговорка)
`getting_started.nv --mode release`, тёплый libuv-кеш:
- default (1 TU, старый путь): **7.77s**
- split (7 частей параллельно, `NOVA_MULTI_TU=1` + сниженный порог): **4.77s**

Split оказался БЫСТРЕЕ даже на этой крошечной CU (7 частей, каждая
&lt;20КБ) — вероятно, доминирует spawn/compile overhead одного clang-
процесса на мелкий файл, не суперлинейность (той, ради которой Plan 209
существует, и которая проявляется только на реально больших CU). Это
НЕ репрезентативная величина ускорения для 13-МБ conformance CU —
только proof-that-it-doesn't-regress-badly на игрушечном входе; реальный
замер — задача Ф.3 (оркестратор, включение флага на mega-CU).

## ⚠ НОВАЯ НАХОДКА (вне периметра Ф.2, НЕ исправлено, для владельца)

При попытке протестировать mono-gap-фикс через РЕАЛЬНЫЙ явный вызов
(`a.merge_from(b)`, `for v in a.values() {}}`, не только косвенный через
`.insert()`) — программа компилируется и ЛИНКУЕТСЯ (и в default, и под
multi-TU), но **ведёт себя некорректно ФУНКЦИОНАЛЬНО**:
- `for v in a.values() { ... }` — тело цикла НИ РАЗУ не выполняется
  (`count` остаётся `0` при непустом Set) — `.values()` не даёт элементов.
- `a.merge_from(b)` — no-op (`a.len()` не меняется после мержа).

Воспроизводится ИДЕНТИЧНО в default (single-TU, НЕ NOVA_MULTI_TU) режиме
— **никак не связано с Plan 209** (ни с Task A тулчейном, ни с Task B
DCE-фиксом — literal-вызов делает имя reachable независимо от Task B).
Это отдельный, ПРЕ-СУЩЕСТВУЮЩИЙ дефект в D39 embed-delegation call-site
диспатче (emit_embed_proxies/связанные call-site пути) — судя по
предварительному разбору (не доводил до конца, вне периметра): вероятно
тот же класс проблемы, что Ф.1-находка описывала как "неопределённость 2"
(`emit_embed_proxies`'s `Nova_Set_method_merge_from(Nova_Set* nova_self,
...)` берёт БАЗОВОЕ (не per-инстанс-мono'д) имя `base_c_name` без
корректной подстановки типа embedded-поля — если это приводит к вызову
НЕ ТОЙ функции/некорректному приведению, поведение будет именно таким:
компилируется, линкуется (раз имя разрешилось хоть на что-то), но делает
не то). **НЕ фиксил** — это call-site/dispatch дефект, не DCE-коллектор
(Task B), и не тулчейн (Task A); фикс вслепую рискован (тот же принцип,
что f1-notes уже сформулировали для оригинальной находки). Рекомендация:
отдельная волна ДО широкого использования D39 embed-делегации с
НЕ-тривиальными (не только `len()`/`contains()`-подобными) методами.

## Коммиты этой волны
- `11ca28768` — Task B (mono-gap DCE-фикс, emit_c.rs).
- `c56069534` — Task A (тулчейн, test_runner.rs + nova-cli/main.rs).
- `a6317dd8c` — 3 split_tu сегментер-бага + 4 regression-теста
  (split_tu.rs), найдены при верификации Task A.
