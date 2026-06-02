# Research Nova

Справочные материалы: сравнения языков, бенчмарки, findings, заметки
под решения. Это **не планы** (что делать) и **не источники истины**
(те живут в `spec/decisions/`/`syntax.md`/etc), а **research-артефакты**.

## Схема нумерации

- `01-…`, `02-…` — research-документы по порядку создания.

## Текущие материалы

| # | Файл | О чём |
|---|---|---|
| 03 | [03-language-comparison-matrix.md](03-language-comparison-matrix.md) | Матрица: Nova vs 9 языков по 10 болям и 10 возможностям |
| 04 | [04-gc-comparison.md](04-gc-comparison.md) | GC: размер кода и runtime overhead (ZGC, Go, .NET, Erlang, OCaml...) |
| 05 | [05-go-mistakes-audit.md](05-go-mistakes-audit.md) | Аудит дизайна Nova по «100 Go Mistakes»: что закрыто, что воспроизводится |
| 06 | [06-field-visibility-go-kubernetes.md](06-field-visibility-go-kubernetes.md) | Field visibility в Go production code: kubernetes statistical audit (35239 fields, 11099 structs; 59% public / 41% private; layer-dependent distribution) — validates Nova D47 public-default |

> Нумерация начинается с 03, потому что 01 и 02 переехали в отдельную
> репу с черновиками публикаций.

## Связанные директории

- [docs/plans/](../plans/) — планы (что и когда делаем)
