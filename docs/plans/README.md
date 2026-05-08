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
| 04 | [04-buffer-split-and-external.md](04-buffer-split-and-external.md) | Buffer → StringBuilder/WriteBuffer/ReadBuffer + `external` keyword | активный, не начат |
| 05 | [05-as-cast-codegen.md](05-as-cast-codegen.md) | `as`-cast — реализация narrowing в codegen (D54 compliance) | активный, не начат |
| 06 | [06-iter-protocol-codegen.md](06-iter-protocol-codegen.md) | `Iter[T]` protocol в codegen — общий for-in (D58 compliance) | активный, не начат |
| 07 | [07-as-cast-saturation.md](07-as-cast-saturation.md) | `as`-cast saturation для float→int + spec D54 narrowing semantics | активный, не начат |

## Связанные директории

- [docs/research/](../research/) — справочные материалы и сравнения
