# PROGRESS — план 241 Ф.1 (английские переводы спеки ru→en)

> Предыдущая задача в этом файле (глоссарий spec/GLOSSARY.en.md, ветка
> `p241-glossary`, план Ф.0) закрыта полностью (см. git log PROGRESS.md) —
> файл переиспользован под новую задачу этого worktree/ветки.

Ветка: `p241-spec`. Работаем только в этом worktree. Компилятор/сборки/тесты
не запускаем. Перевод — informative, русский норматив.

Порядок: overview → conversions → effects → revolutionary → syntax (по H2-секциям).

- [x] `spec/overview.md` → `spec/overview.en.md` (257 строк) — 1 коммит
- [x] `spec/conversions.md` → `spec/conversions.en.md` (481 строк) — по H2-секциям
- [x] `spec/effects.md` → `spec/effects.en.md` (413 строк) — по H2-секциям
- [ ] `spec/revolutionary.md` → `spec/revolutionary.en.md` (945 строк) — по H2-секциям
- [ ] `spec/syntax.md` → `spec/syntax.en.md` (1792 строк) — по H2-секциям (~15 коммитов)

Пометки о сделанном — ниже, по мере продвижения.

## Статус

**overview.en.md — ГОТОВО** (коммит `6aa442742`). Вся страница одним
куском, сразу после первого перевода закоммичена и запушена на
`p241-spec`. Source: `spec/overview.md`, source_rev `615e2fa7e`
(2026-08-02). Далее: conversions (по H2-секциям).

**conversions.en.md — ГОТОВО.** Переведено по H2-секциям: Три механизма
(`c4d59af5e`), Numeric ↔ numeric (`79987349d`), Numeric ↔ str (`ad7004d6d`),
Char/Byte/[]byte/str (`461b9df3e`), Bool+Newtype (`6bf0918e6`),
Sum-variant+Strict cond (`92e07537f`), #coerce (`9826dd8c6`),
from/try_from naming (`b4e120175`), Precedents+Status+References (`43a2dd003`).
Source_rev `e5b206e36` (2026-07-26). Далее: effects (по H2-секциям).

**effects.en.md — ГОТОВО.** Коммиты: `92df8a7ed` (Центральный принцип +
Эффект = интерфейс), `b4ade0b5e` (Syntax + Names + Positions + Standard
effects), `c9b03faae` (Why needed + Direct effects), `48e638347` (Async +
Default handler), `8bb87da95` (Panic + Roles + Operators + Result + Main
point). Source_rev `337ec42af` (2026-07-26). Далее: revolutionary (по
H2-секциям).
