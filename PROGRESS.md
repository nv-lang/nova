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
- [x] `spec/revolutionary.md` → `spec/revolutionary.en.md` (945 строк) — по H2-секциям
- [x] `spec/syntax.md` → `spec/syntax.en.md` (1792 строк) — по H2-секциям (10 коммитов)

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

**revolutionary.en.md — ГОТОВО.** Коммиты: `79b5fb554` (§R1), `a7bf71596`
(§R2–R4), `10b2f3bd8` (§R5.1–R5.2), `3ee1d9b43` (§R5.3–R5.6), `03dc2aaf5`
(§R5.7), `7e06d8612` (§R5.7 rest + §R6), `301e63035` (§R7–R9), `b888c8234`
(§R10–R11), `8836f4a8f` (§R12 part 1), `c7ab52634` (§R12 rest + final).
Source_rev `dcdf639fa` (2026-05-31). Далее: syntax (по H2-секциям, ~15
коммитов).

**syntax.en.md — ГОТОВО.** Коммиты: `721453ded` (§Минимальные примеры +
§Tagged templates), `374108a77` (§String interpolation — §Closure),
`d8674f20b` (§Trailing + §Function body), `b9713b13c` (§Operator overloading
— §Naming conventions), `6dd74579f` (§Visibility + §Type declarations +
§Creating values), `a24c14bdc` (§Methods + §Embed/delegation),
`b70ba9df1` (§Params + §Optional params + §Effects + §Contracts + §Handlers
+ §With + §Concurrency), `9eaea2667` (§Capability + §Perf + §Protocol +
§Generics + §Bounds + §Conversions + §spawn/supervised), `5cac9ff1b`
(§supervised + §parallel for + §detach + §Channel/select + §Time.sleep +
§Testing + §Panic). Source_rev `615e2fa7e` (2026-08-02). Все 36 H2-секций
совпадают с оригиналом. Задача закрыта.
