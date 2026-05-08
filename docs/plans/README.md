# Планы Nova

В этой директории — только **планы** (что и когда делаем). Справочные
материалы (таблицы сравнений, research-заметки, бенчмарки) живут в
[docs/research/](../research/).

## Схема нумерации

- `01-…`, `02-…` — главные планы по порядку создания.

## Текущие планы

| # | Файл | О чём | Статус |
|---|---|---|---|
| 01 | [01-roadmap-v0.1.md](01-roadmap-v0.1.md) | Roadmap разработки компилятора v0.1–v1.0+ | активный |
| 02 | [02-codegen-c-backend.md](02-codegen-c-backend.md) | C backend: компиляция Nova в нативный бинарь | активный |
| 03 | [03-package-ecosystem-roadmap.md](03-package-ecosystem-roadmap.md) | Package ecosystem: self-host → CLI → lockfile → registry | будущий (после v2.0+) |
| 04 | [04-buffer-split-and-external.md](04-buffer-split-and-external.md) | Buffer → StringBuilder/WriteBuffer/ReadBuffer + `external` keyword | ✅ выполнено (Buffer удалён из языка) |
| 05 | [05-as-cast-codegen.md](05-as-cast-codegen.md) | `as`-cast — реализация narrowing в codegen (D54 compliance) | ✅ выполнено |
| 06 | [06-iter-protocol-codegen.md](06-iter-protocol-codegen.md) | `Iter[T]` protocol в codegen — общий for-in (D58 compliance) | ✅ выполнено |
| 07 | [07-as-cast-saturation.md](07-as-cast-saturation.md) | `as`-cast saturation для float→int + spec D54 narrowing semantics | ✅ выполнено |
| 08 | [08-from-into-conversions.md](08-from-into-conversions.md) | `From`/`Into` framework + char/byte/bool + strict if-cond + conversions.md | частично закрыт (Ф.1-Ф.5+Ф.7), осталось Ф.6+Ф.8-Ф.9 |
| 09 | [09-clang-migration.md](09-clang-migration.md) | Миграция Windows-сборки с MSVC на Clang/LLVM (10-15% perf) | активный, не начат |
| 10 | [10-pgo-integration.md](10-pgo-integration.md) | PGO integration (stub, после плана 09) — 15-30% perf на hot path | stub / future |
| 11 | [11-method-values-and-overload.md](11-method-values-and-overload.md) | Method values + overload по типу аргумента (закрывает Q-overloading вариант 1) | частично закрыт (Ф.1-Ф.3+Ф.4.5+Ф.6+Ф.7), осталось Ф.4+Ф.5+Ф.8 |
| 12 | [12-builtins-driven-codegen.md](12-builtins-driven-codegen.md) | builtins.nv-driven external dispatch + auto-derive try_*/into() из Fail-form/from() (Q-codegen-builtins-cleanup) | ✅ ЗАКРЫТ (кроме Ф.6 — отложен) |
| 13 | [13-runtime-stdlib-and-autogen.md](13-runtime-stdlib-and-autogen.md) | Runtime stdlib (str/math) + auto-gen std/runtime/*.nv (read-only projection реестра компилятора) | MVP ✅ + Ф.8 ✅ (декомпозиция builtins.nv); осталось Ф.4 + Ф.9 (API polish: chaining `Self`, op+, char_len→len, []char) |

## Связанные директории

- [docs/research/](../research/) — справочные материалы и сравнения
