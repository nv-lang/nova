# Промпт: прочитай проект

**Используй эту фразу в начале любой сессии над nova-lang.** (Актуализировано 2026-07-13.)

Читай в следующем порядке:

---

> **СНАЧАЛА (онбординг + жёсткие правила):** [`docs/dev/dev-workflow.md`](../dev-workflow.md) — как устроена
> разработка (план-ориентированный процесс, worktree-модель, daily loop) и **жёсткие операционные правила**
> (никакого `git stash` — baseline через temp-worktree/commit-reset; `git add` только по именам файлов;
> греп конфликт-маркеров ОДНОЙ командой с коммитом; коммит на задачу; без AI co-author trailer'ов;
> язык-меняющее слияние не пушится без спек-амендмента в том же слиянии). Точка входа для агентов —
> [`AGENTS.md`](../../../AGENTS.md). **Прочитай ДО того, как брать задачу** — эти правила перекрывают любой
> устаревший текст в других доках.

---

## 0. Что это за проект (одним абзацем)

Nova — язык с эффектами и structured concurrency, компилируется **только через C-codegen**
(интерпретатора НЕТ; `nova run` ретрактирован — тестируем через `nova test`/`test-build`).
Компилятор — Rust (`compiler-codegen/` + `nova-cli/`), std — на самой Nova (`std/*.nv`).
Спека = D-блоки в `spec/decisions/`. Правило №1: **тест авторитетен** — если фича не работает,
чинится компилятор в правильном месте; тесты не ослабляются и не удаляются.

`nova.toml` — workspace-конфиг (members: std/, examples/, spec_tests/). Внешние native-модули —
отдельные репы-сиблинги (эталон: `../nova-tls`, план 195/193: `.nv`-фасад + vendored C, ноль Rust).

## 1. Спека языка (`spec/`)

```
spec/overview.md          — центральная идея, killer use-case, trade-offs
spec/syntax.md            — грамматика, ключевые слова, литералы
spec/decisions/           — все D-блоки (01-philosophy ... 09-tooling)
spec/open-questions.md    — что ещё не решено (Q-реестр)
```

Особое внимание: `04-effects.md` (эффекты/Fail/handler'ы), `02-types.md` (типы/протоколы/generics/D55-коэрсия),
`07-modules.md` (D78, папка = ОДИН модуль из co-equal файлов), `03-syntax.md` (D406 `enum`-маркер сумм,
D48 tag-шаблоны). **Не выдумывай синтаксис** — сверяйся со спекой и `examples/`.

## 2. Текущее состояние и куда двигаться (2026-07-13)

Статусы планов — **только** [`docs/plans/README.md`](../../plans/README.md) + сами планы. Главное сейчас:

- **Plan 196 «одно окно правды» — ВЫСШИЙ приоритет.** Чекер резолвит ОДИН раз → каналы
  (`resolved_types: ExprId→ResolvedType`, `resolved_callees`) → codegen ЧИТАЕТ (`resolved_type_to_c`),
  а не перевыводит. Две встречные волны: [196.2](../../plans/196.2-class-c-relocation.md) (волна-1: снятие
  веток `infer_call_ret_c`, emit_c.rs 46293-48883 — 26/114 снято, остался carrier-chain/финал) и
  [196.3](../../plans/196.3-wave2-d-driven.md) (волна-2: миграция сиблинг-функций по D — 12/12 инвентаря
  обработаны, трекер с колонками «Закрыто в / Одно окно ✔ / Доказательство»). Фундамент —
  [196.4](../../plans/196.4-call-resolvedtype-channel.md): Stage-1a+1b ✅ (канал материализует
  method-generic и static-generic возвраты, гейт propose-then-verify). **Следующий keystone =
  node_substs-канал (Stage-1c)** — разблокирует Tier-2 (d119/d122/d30/d85) и финальный коллапс
  `infer_call_ret_c`. Приёмка любого закрытия — ПО КОДУ, с доказательством в трекере 196.3.
- **Plan 200** — живой реестр std-улучшений (`Vec.new(cap)` ✅, `new(ptr,len,cap)` ✅, миграция
  `.new().cap()` ✅; в очереди П6 `Vec.data→ptr`; П3 As*-протоколы — в Q).
- **Plan 187** — флагман-агрегатор: MVP ✅ (`examples/flagship/aggregator/`); остаток: SSE-мост,
  typed-serde, real-cancel (за 173), Live-источники. Фронт = мокап
  `docs/research/assets/15-showcase-mockup.html` ВЕРБАТИМ.
- **Plan 173** — runtime-хвосты: `[M-parfor-record-result-miscompile]` и `supervised(deadline:)`
  (включая гонку «sleep не прерывается») — в работе, срочные.
- **Plan 193** — ✅ закрыт (nova-tls = внешний dep); хвост: vendored-сборка mbedTLS (195-паттерн по
  прецеденту libuv build-and-cache).
- **Plan 198** — nova_tests-триаж: DELETE ✅; MIGRATE (в std/**/*_test.nv рядом с модулем /
  spec_tests/conformance / spec_tests/soundness) — финиширует. **nova_tests заморожен** — новые
  тесты туда не пишутся.
- Открытые `[M-…]`-маркеры — [backlog-followups.md](../../plans/backlog-followups.md) (в т.ч. свежий
  кластер `[M-flagship-*]` и P67-LEGACY-класс).

## 3. Инструменты (`docs/dev/promts/read-toolchain.md`)

```sh
# собрать (release ОБЯЗАТЕЛЬНО — debug на порядок медленнее из-за vcvars)
cd compiler-codegen && cargo build --release && cd ..
cd nova-cli && cargo build --release && cd ..

# ГЛАВНЫЙ ГЕЙТ: conformance — ОДИН compile unit (не per-file!)
nova-cli/target/release/nova test --positive --compile-error --timeout 300 --jobs 4 spec_tests/conformance

# std-тесты (полный прогон долгий — обычно таргетно по папкам)
# Plan 195: std на src/ — реальный путь std/src/<домен> (module-path не меняется).
nova-cli/target/release/nova test std/src/collections std/src/data
nova-cli/target/release/nova test --filter X ; nova test --rerun-failed ; nova test --jobs 1

# один файл (интерпретатора НЕТ — только build/check)
nova-cli/target/release/nova build <file.nv>
nova-cli/target/release/nova check <file.nv>
```

### Сборка вне главной репы (worktree, nova-http/tls/polaris/compress) — БЕЗ КОПИРОВАНИЯ

**НИЧЕГО НЕ КОПИРОВАТЬ.** Все пути указываются переменными на главную репу — блок ниже
копируется целиком (замени `<main>` на путь к репе `nova`):

```sh
M=<main>                      # напр. D:/Sources/nv-lang/nova
export NOVA_STD_PATH="$M/std/src"
export NOVA_RT_DIR="$M/compiler-codegen/nova_rt"
export NOVA_CG_INCLUDE="$M/compiler-codegen"
export NOVA_GC_LIB_DIR="$M/compiler-codegen/vcpkg_installed/x64-windows-static/lib"
export NOVA_INCLUDE_DIR="$M/compiler-codegen/vcpkg_installed/x64-windows-static/include"
export NOVA_GC_INCLUDE_DIR="$NOVA_INCLUDE_DIR"
```

**ЗАПРЕЩЕНО копировать `compiler-codegen/` (рантайм) внутрь пакетной репы или worktree.**
Копия не под git → её протухание НЕВИДИМО, и она ШАДОВИТ настоящий рантайм. Реальная цена
промаха (2026-07-27, реестр 221.1 №138): полтора часа диагностики «регрессии компилятора» в
nova-http, которой не было — заголовки копии были сняты ДО фикса №108 и не объявляли символ,
который компилятор уже эмитит; плюс больше гигабайта мусора по репам (526 МБ в polaris,
331 МБ в одном worktree). Проверено: polaris `nova test src --strict-effects` даёт 35/0/16
БЕЗ всякой копии, только на переменных выше.

Машинный страж: `scripts/guards/check-no-runtime-copy.sh` (в `gate.sh`; самотест —
`scripts/guards/selftest/test-check-no-runtime-copy.sh`). Детали и прочие ловушки — `read-toolchain.md`.

## 4. Состояние тестов (baseline 2026-07-13)

- `spec_tests/conformance` (один CU): **97 PASS / 0 FAIL** — красный conformance = стоп-сигнал.
- `nova test std` (Plan 195, std на `src/`, path `std/src/<домен>`): **63 PASS / 2 известных FAIL**
  (`concurrency/retry_test` CC-FAIL — генерик-моно codegen `nova_str`↔`Nova_T*`, не path-related;
  `time/units_test` — sleep-timing флака, `elapsed.as_millis() >= 50`) / 66 SKIP (библиотечные файлы
  без test-блоков — норма). TIMEOUT'ы под `--jobs 16` при параллельной нагрузке — известная флака:
  перепроверяй изолированно `--jobs 1..4` прежде чем считать регрессией.
- `nova_tests/` — НЕ гейт корректности (заморожен, мигрируется планом 198).

Перед работой сверь baseline; твоя задача не должна добавить FAIL'ов.

## Что НЕ читать сразу

- `compiler-codegen/src/` — только файлы, релевантные задаче (emit_c.rs — 50k+ строк).
- `docs/project-creation.txt`, `docs/dev/simplifications.md` — исторические логи.
- `docs/research/` — справочные материалы, не планы.
