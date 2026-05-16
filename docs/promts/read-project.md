# Промпт: прочитай проект

**Используй эту фразу в начале любой сессии над nova-lang.**

Читай в следующем порядке:

---

## 0. Корневой конфиг (`nova.toml`)

`nova.toml` — workspace-конфиг Nova. Содержит `[workspace]` с members (std/, examples/, nova_tests/).
Читай первым — это точка входа в структуру проекта.

---

## 1. Спека языка (`spec/`)

```
spec/overview.md          — центральная идея, killer use-case, trade-offs
spec/syntax.md            — грамматика, ключевые слова, литералы
spec/decisions/           — все D-блоки (01-philosophy ... 09-tooling)
```

Особое внимание:
- `spec/decisions/04-effects.md` — эффекты, Fail, handler'ы, D62/D64/D85/D91/D92/D93
- `spec/decisions/02-types.md` — типы, протоколы, generics, D42/D54/D66/D72
- `spec/decisions/07-modules.md` — D78 path/module enforcement (имя директории = имя пакета)
- `spec/open-questions.md` — что ещё не решено

---

## 2. Текущие планы (`docs/plans/README.md`)

Читай таблицу планов целиком — статусы меняются. Активные и высокоприоритетные:

- **Plan 27** (`27-gc-switch.md`) — GC switch: Boehm как default. **Ф.1-Ф.4 выполнены.** Boehm — default GC.
- **Plan 31** (`31-select-statement.md`) — `select` statement (мультиплексирование каналов). В работе.
- **Plan 19** (`19-closure-and-error-ops.md`) — closure-rev + D85 error-ops.
- **Plan 20** (`20-defer-implementation.md`) — `defer`/`errdefer`.

Прочитай план(ы) релевантные задаче целиком.

---

## 3. Инструменты (`docs/promts/read-toolchain.md`)

Структура репо, nova CLI, как запускать тесты, как добавлять тесты,
ключевые ловушки test_runner API.

Краткая шпаргалка:

```sh
# собрать nova CLI (один раз или после изменений компилятора)
cd compiler-codegen && cargo build --release && cd ..
cd nova-cli && cargo build --release && cd ..

# запустить все тесты (release build — в ~70 sec на 16 ядрах)
nova-cli/target/release/nova test

# subset / rerun / sequential
nova-cli/target/release/nova test --filter X
nova-cli/target/release/nova test --rerun-failed
nova-cli/target/release/nova test --jobs 1

# скомпилировать / запустить один файл
nova-cli/target/release/nova build nova_tests/basics/literals.nv
nova-cli/target/release/nova run   nova_tests/basics/literals.nv
nova-cli/target/release/nova check nova_tests/basics/literals.nv

# регенерировать runtime stubs
nova-cli/target/release/nova regen-runtime
nova-cli/target/release/nova regen-runtime --check
```

**ВАЖНО: использовать release-сборку.** Debug-сборка nova-cli/nova-codegen существенно медленнее
из-за инициализации vcvars (6 sec) которая происходит на каждый test-build в debug-режиме.
В release-сборке vcvars кэшируется один раз, каждый тест занимает ~2-3 сек.

---

## 4. Состояние тестов

Перед началом работы прогони тесты чтобы знать baseline:

```sh
nova-cli/target/release/nova test
```

Baseline (2026-05-16): **509 PASS / 26 FAIL / 35 SKIP** (~10 мин на 16 ядрах).

26 known FAIL'ов до этой работы (НЕ регрессии новой задачи):
- 13× `doc/fixtures/*` — Plan 45 фикстуры без `main`, test runner подбирает по ошибке (выносить отдельно)
- 9× `negative_capability/p50_*` — expectation drift Plan 50 (ждут паттерн «передаётся только по имени», текст диагностики изменился)
- 3× прочие expectation drift'ы: `contracts_decreases_recursion_fail`, `fail_handler_no_exit_rejected`, `np_trailing_double_bind`
- 1× `concurrency/fn_array_generic_smoke` — `[]fn->T` для T=int возвращает `.len() == 4` (вероятно related Plan 48 monomorphization in-progress)

Перед началом работы прогони baseline и сверь — твоя задача не должна добавить FAIL'ов.

---

## Что НЕ читать сразу

- `compiler-codegen/src/` — читай только файлы релевантные задаче.
- `docs/project-creation.txt` и `docs/simplifications.md` — исторические логи,
  нужны только если ищешь контекст конкретного решения.
- `docs/research/` — справочные материалы, не планы.
