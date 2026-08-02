# PROGRESS — план 241, Ф.0 (глоссарий spec/GLOSSARY.en.md)

> Предыдущая задача в этом файле (перевод ///-комментариев std) закрыта
> полностью (см. git log PROGRESS.md, коммит 192b73d5d) — файл переиспользован
> под новую задачу этого worktree/ветки.

Ветка: `p241-glossary`. Исполнитель: sonnet. Файл: `spec/GLOSSARY.en.md`.

## Источники (сверены)
- spec/overview.md, spec/paradigm.md (устарел, помечен предупреждением в
  шапке — термины trait/impl НЕ брать как актуальные, актуально
  protocol/effect), spec/syntax.md, spec/effects.md, spec/conversions.md,
  spec/revolutionary.md
- spec/decisions/README.md — тематические разделы (для структуры глоссария)
- docs/guide/language-tour.md, value-vs-reference.md, consume-types.md,
  contracts.md, parameters.md, channels.md, typed-pointers.md — источник
  устоявшегося английского словоупотребления

## Разделы (чекпоинты)
- [x] 1. Философия и парадигма эффектов
- [ ] 2. Ключевые слова/идентификаторы (не переводятся)
- [ ] 3. Типы и данные
- [ ] 4. Связывания, владение, сопоставление с образцом
- [ ] 5. Эффекты и обработка ошибок
- [ ] 6. Память и производительность
- [ ] 7. Конкурентность (рантайм Vela)
- [ ] 8. Модули и пакеты
- [ ] 9. Рантайм, FFI и unsafe
- [ ] 10. Тулинг и контракты
- [ ] 11. Конверсии и перегрузка
- [ ] Open questions for owner review (финализация)

## Найденные [proposed]-термины (пока)
- «одна дверь» → single canonical path / "no second door" [proposed]
- «скрутини» → scrutinee [proposed] (стандартный PL-термин, но не
  засвидетельствован в доке Nova — сейчас просто "the value being matched")

## Заметка
paradigm.md помечен УСТАРЕВШИМ в шапке файла (описывает `trait`/`impl`,
которые заменены на `protocol`/эффект-через-kind-токен) — термины оттуда
в глоссарий не тащу как актуальные, только в раздел keywords если понадобится
историческая сноска.
