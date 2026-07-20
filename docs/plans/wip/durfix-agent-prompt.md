<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# Промпт волны `durfix` — 3 Duration/standalone-CU маркера (модель: sonnet)

Передать агенту как есть. После слияния волны файл удалить.

---

РОЛЬ: codegen/checker-фиксер компилятора Nova. Волна из ТРЁХ родственных маркеров
хрупкости Duration/standalone-CU-путей. Работаешь ТОЛЬКО в своём worktree.
Модель прогона: sonnet — укажи её в отчёте.

ПЕРВАЯ КОМАНДА: `git -C d:/Sources/nv-lang/nova worktree add ../nova-durfix main`
Дальше ВСЁ — в `d:/Sources/nv-lang/nova-durfix` (каждая команда с абсолютным cd,
проверяй pwd). В main-репу не заходить, main не трогать, не пушить — слияние и
авторитетный гейт делает интегратор.

СБОРКА/ТЕСТ (env обязателен, из worktree):

```
export NOVA_GC_LIB_DIR=$PWD/compiler-codegen/vcpkg_installed/x64-windows-static/lib
export NOVA_INCLUDE_DIR=$PWD/compiler-codegen/vcpkg_installed/x64-windows-static/include
export NOVA_GC_INCLUDE_DIR=$NOVA_INCLUDE_DIR
cargo build --release --manifest-path nova-cli/Cargo.toml
./nova-cli/target/release/nova build <file> -o <out> --strict-effects
```

## МАРКЕР 1 (P2): `[M-parfor-capture-callee-name-collides-std-local]`

Строка в `docs/plans/backlog-followups.md` (~строка 30). Суть: захват-анализ
spawn-ctx у `parallel for` кладёт в ctx СВОБОДНОЕ ИМЯ ВЫЗЫВАЕМОЙ module-fn, если
у ЛЮБОЙ другой функции CU есть ЛОКАЛ с тем же именем (тип берётся у чужого локала!).

РЕПРО (детерминированный): в standalone-CU объяви module-fn с именем `probe`
(коллизия с локалом `uint64_t probe = mag` std-движка float-форматирования) и
вызови её из тела `parallel for`. Возьми `examples/mini_aggregator.nv` и
переименуй `ask_source`→`probe`: CC-FAIL `use of undeclared identifier 'probe'`
на `_nova_spawn_1_ctx->probe = probe;` — при том что САМ вызов эмитится правильно
(mangled, в drain-fn); поле ctx чисто паразитное.

ЗОНА: emit_c, сбор свободных идентификаторов для spawn-ctx (семья `NovaSpawnCtx_*`).
КОРЕНЬ: lookup по голому имени без скоупа — та же болезнь, что закрытая 196.6
(`closure_param_type_overrides` per-name без per-fn).

ФИКС ПО СУЩЕСТВУ: имена, резолвящиеся в module-fn (вызываемая позиция), НЕ
захватываются как переменные — исключить из capture-набора по РЕЗОЛВУ (resolved
callee), не по эвристике имени. НЕ лечить пер-именным блэклистом.

## МАРКЕР 2 (P3): `[M-vr-binop-wrapper-decl-order-standalone-cu]`

Суть: `t0 + 120.to_millis()` (Monotonic + Duration, value-record binop) в
standalone-CU эмитит обёртку `nova_vr_binop_Nova_Monotonic_method_plus(...)
{ return Nova_Monotonic_method_plus(&a,b); }` ДО объявления
`Nova_Monotonic_method_plus` → implicit int → CC-FAIL `returning 'int' from a
function with incompatible result type 'NovaValue_Monotonic'`. В много-файловом
CU (examples/flagship/aggregator: `ro shared_deadline Monotonic = t0 + budget`) —
собирается: чистая ошибка порядка деклараций.

РЕПРО: в mini_aggregator.nv заменить `supervised(timeout: 120.to_millis())` на
`supervised(deadline: t0 + 120.to_millis())` (t0 передать параметром) — или
минимальный standalone: `ro d Monotonic = Monotonic.now() + 5.to_millis()`.

ФИКС ПО СУЩЕСТВУ: vr_binop-обёртки — в единую точку splice ПОСЛЕ деклараций
методов. ПРЯМОЙ ОБРАЗЕЦ — закрытый `[M-tuple-fixarr-typedef-order]`, коммит
`2f5128367` (единый топо-DAG в одну точку splice, ленивые заголовки → байт-паритет
частого случая). Прочитай тот дифф ПЕРЕД проектированием.

## МАРКЕР 3 (P3): `[M-p67-path-call-const-receiver-method-ice]` (+ брат)

Суть: `const BUDGET_MS int = 120` + `BUDGET_MS.to_millis()` → ICE `[P67-LEGACY]
Path call return type unknown for method=to_millis — checker must annotate`
(emit_c.rs:52880). Литерал `120.to_millis()` работает; const-идентификатор-ресивер
generic-extension (`fn[T Ints] T @to_millis()`, std/src/time/duration/core.nv) — ICE.

БРАТ той же семьи: `[M-flagship-monotonic-now-bare-binding-ice]` —
`ro t0 = Monotonic.now()` БЕЗ аннотации → тот же класс ICE (задокументирован в
`examples/flagship/aggregator/src/app/aggregate.nv` ~строка 191, репро
«bare 5-line probe»).

ФИКС ПО СУЩЕСТВУ — В ЧЕКЕРЕ (rustc-эталон: типы резолвит чекер, codegen не
гадает): чекер обязан аннотировать return-тип path/method-вызова в обоих shapes;
ICE в emit_c должен стать недостижим для них. НЕ добавлять codegen-side угадайку.
Если за shapes одна общая дыра аннотирования — почини дыру, оба маркера
закроются одним фиксом.

## ГЕЙТЫ ВОЛНЫ (все обязательны, в отчёт — полные вердикты)

1. `mini_aggregator.nv` в ЧЕТЫРЁХ формах собирается и бежит (3× прогон каждой,
   `done=4 cancelled=2`):
   - (а) текущая (ask_source + `timeout:`) — регресс;
   - (б) fn переименована в `probe` — маркер 1;
   - (в) `deadline: t0 + 120.to_millis()` (t0 параметром) — маркер 2;
   - (г) `const BUDGET_MS int = 120` + `BUDGET_MS.to_millis()` И
     `ro t0 = Monotonic.now()` без аннотации — маркер 3 (оба shape).
2. Флагман: `nova build examples/flagship/aggregator/src/main.nv --strict-effects`
   — зелёный.
3. Байт-паритет `.c` на 2-3 НЕ-затронутых фикстурах
   (`spec_tests/conformance/tuple_fixarr_typedef.nv` +
   `d55_bytes_lit_type_directed.nv`): дифф пуст или объясним построчно
   (ленивые заголовки как в 2f5128367 — ОК).
4. Таргетные тесты: `nova test std/src/time/duration`;
   `nova test std/src/concurrency` (supervised_deadline_test обязателен зелёным).
5. НИКАКОГО мега-CU/полного conformance — авторитетный гейт прогоняет интегратор.
6. ФИНАЛЬНЫЙ ШАГ ВОЛНЫ: mini_aggregator.nv → канон: `deadline:`-форма D408 (одна
   абсолютная точка, t0 параметром) + `const BUDGET_MS`; имя `ask_source`
   ОСТАВИТЬ (читабельнее probe); снести обходные комментарии-ссылки на маркеры
   2/3; коммент про маркер 1 заменить на «имя fn свободное — коллизия имён
   починена [этой волной]». Три строки маркеров в backlog-followups.md →
   перенос в `docs/history/simplifications-closed.md` одной записью волны
   (lifecycle §13); у `[M-flagship-monotonic-now-bare-binding-ice]` обновить
   комментарий в aggregate.nv (аннотации `ro t0 Monotonic` НЕ снимать —
   безвредны, byte-churn не нужен); хвост «Известного расхождения» в
   `docs/nv-coding-style.md` §31 обновить (маркеры закрыты; миграция флагмана
   на Duration разблокирована, отдельная волна).

## ОПЕРАЦИОНКА (жёстко)

- Синтаксис Nova не выдумывать: spec/decisions/ + examples/ + существующие фикстуры.
- Не спавнить суб-агентов. CPU-дисциплина: никаких параллельных тяжёлых сборок.
- При rate-limit — чекпоинт прогресса в `docs/plans/wip/durfix-progress.md`
  (в worktree), мелкие батчи, продолжение оттуда.
- `git add` ТОЛЬКО по именам файлов; греп конфликт-маркеров ОДНОЙ командой с
  коммитом; перед commit — `git diff --cached --stat`; без stash; без Co-Authored-By.
- Коммиты в свою ветку worktree; НЕ push, НЕ merge.
- Тест авторитетен: не ослаблять, не удалять; чинится компилятор в правильном месте.

## ОТЧЁТ

1. По каждому маркеру: корень (файл:строка) → фикс (файл:строка) → почему по существу.
2. Вердикты всех гейтов 1-4 (полные, не «ок»).
3. Изменённые файлы + хеши коммитов ветки.
4. Что НЕ удалось / OPEN-вопросы (упёрся — hard-stop с диагнозом, не обходка).
5. Модель: sonnet.
