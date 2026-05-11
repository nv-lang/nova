# Промпт: прочитай проект

**Используй эту фразу в начале любой сессии над nova-lang.**

Читай в следующем порядке:

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

- **Plan 27** (`27-gc-switch.md`) — GC switch: Boehm как default. **Высокий приоритет, не начат.**
- **Plan 19** (`19-closure-and-error-ops.md`) — closure-rev + D85 error-ops.
- **Plan 20** (`20-defer-implementation.md`) — `defer`/`errdefer`.
- **Plan 21** (`21-channel-revision-implementation.md`) — Channel revision.

Прочитай план(ы) релевантные задаче целиком.

---

## 3. Инструменты (`docs/promts/read-toolchain.md`)

Структура репо, nova CLI, как запускать тесты, как добавлять тесты,
ключевые ловушки test_runner API.

Краткая шпаргалка:

```sh
# собрать nova CLI (один раз или после изменений компилятора)
cd nova-cli && cargo build && cd ..

# запустить все тесты
nova-cli/target/debug/nova test

# subset / rerun / sequential
nova-cli/target/debug/nova test --filter X
nova-cli/target/debug/nova test --rerun-failed
nova-cli/target/debug/nova test --jobs 1

# скомпилировать / запустить один файл
nova-cli/target/debug/nova build nova_tests/basics/literals.nv
nova-cli/target/debug/nova run   nova_tests/basics/literals.nv
nova-cli/target/debug/nova check nova_tests/basics/literals.nv

# регенерировать runtime stubs
nova-cli/target/debug/nova regen-runtime
nova-cli/target/debug/nova regen-runtime --check
```

---

## 4. Состояние тестов

Перед началом работы прогони тесты чтобы знать baseline:

```sh
nova-cli/target/debug/nova test
```

Запомни N/N PASS — не сломай регрессию.

---

## Что НЕ читать сразу

- `compiler-codegen/src/` — читай только файлы релевантные задаче.
- `docs/project-creation.txt` и `docs/simplifications.md` — исторические логи,
  нужны только если ищешь контекст конкретного решения.
- `docs/research/` — справочные материалы, не планы.
