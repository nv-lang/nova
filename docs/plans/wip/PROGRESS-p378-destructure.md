# PROGRESS: p378-consume-destructure (№378, `[M-73.1-destructure]`)

Модель: sonnet. Ветка `p378-consume-destructure`, worktree `nova-p378`.

## Итог

**Обе формы заработали.** Круглая `consume (a, b) = pair` (positional tuple)
и фигурная `consume {a, b} = rec` (record / named tuple, by-name) оба
парсятся и проходят весь канал (парсер → чекер → codegen) без единой
дополнительной строки за пределами парсер-гейта.

## Где правился парсер

Ровно один match-arm в `parse_stmt_or_expr`
(`compiler-codegen/src/parser/mod.rs`, было на строке 11583):

```rust
TokenKind::KwConsume if matches!(self.peek_at(1).kind, TokenKind::Ident(_) | TokenKind::KwMut) => {
```

расширен на:

```rust
TokenKind::KwConsume if matches!(
    self.peek_at(1).kind,
    TokenKind::Ident(_) | TokenKind::KwMut | TokenKind::LParen | TokenKind::LBrace
) => {
```

Это единственная правка компилятора. Причина, почему этого хватило:
`parse_consume_decl_or_scope` (вызывается этим армом) уже безусловно звал
общий `self.parse_pattern()`, который строит `Pattern::Tuple`/
`Pattern::Record` ОДИНАКОВО для `ro`/`mut`/`consume` — сам разбор паттерна
никогда не был специфичен для `consume`. Единственное, что блокировало
`consume (`/`consume {`, — сам входной гейт диспетчера statement'ов,
не пускавший `(`/`{` дальше первого токена lookahead.

Специальные интерцепты внутри `parse_consume_decl_or_scope` (re-consume
блок `consume X { … }`, multi-var `consume A, B, C { … }`) устроены на
`TokenKind::Ident(_)`-проверке ПЕРЕД вызовом `parse_pattern()` — при
входном `(`/`{` эта проверка не совпадает и корректно проваливается в
общий `parse_pattern()` путь. Никакой коллизии с существующими формами.

## Переиспользована ли p364-машинерия

**Да, полностью, без единой правки.** Три независимых канала уже
существовали ДО этого окна и просто заработали, когда парсер начал
пропускать паттерны после `consume`:

1. **Per-элементная consume-обязательность (D133).**
   `consume_walk_stmt`'s `Stmt::Let`-ветка (`types/mod.rs`) уже вызывает
   `consume_pattern_names(&decl.pattern, &mut names)` — эта функция уже
   рекурсивно обходит `Pattern::Tuple`/`Pattern::Record` (и `Array`/`Or`)
   и извлекает ВСЕ связанные имена, независимо от формы паттерна. Цикл
   `for (i, n) in names.iter().enumerate() { if decl.consume {
   ctx.declare_consume_binding(n, t); } }` уже был написан без ограничения
   `names.len() == 1`. Он обслуживал `ro`/`mut`-tuple/record-destructure
   (напр. канонический Channel-пример `ro (tx, rx) = Channel.new(n)`,
   `ro {line, col, ..} = @` в json.nv) — просто не мог сработать для
   `consume`, потому что парсер не пропускал туда `Pattern::Tuple`/
   `Pattern::Record` вообще.

2. **Fiber-safety гейт (E_LINEAR_CAPTURE_IN_FIBER, №364 precedent).**
   `walk_stmt`'s `Stmt::Let`-регистрация в `CapState` (`types/mod.rs`
   ~28658-28677) уже содержит `linear_pattern: pat_consume || d.consume`
   — комментарий на месте (написан №364, ДО этого окна) прямо объясняет:
   «`consume lst = expr` (no static annotation, `d.consume`) — LetDecl-
   level explicit-`consume` keyword теперь так же авторитетен, как
   pattern sub-bind's own `is_consume`». Это писалось С РАСЧЁТОМ на
   будущую `consume`-tuple-деструктуризацию, хотя парсер её ещё не
   пропускал. Проверено пробой (ниже) — сработало без правок.

3. **Позиционная форма на именованном кортеже — ошибка (№145).**
   `check_positional_destructure_on_named_tuple`
   (`f1_stmt`'s `Stmt::Let`-обработка, `types/mod.rs`) вызывается
   БЕЗУСЛОВНО для любого `Stmt::Let` (не завязано на `decl.consume`) —
   `consume (a, b) = <именованный кортеж>` получил ту же диагностику
   `E_NAMED_TUPLE_POSITIONAL_DESTRUCTURE`, что и `ro`/`mut`, без единой
   новой строки.

**Своя машинерия не понадобилась вообще.** Единственный НЕ переиспользуемый
кусок — `auto_cleanup_qualifies` в `emit_c.rs` (Plan 217 авто-`errdefer`
для guard-типов) гейтится на `Pattern::Ident` и остаётся `None` для
Tuple/Record-деструктур — это ДОКУМЕНТИРОВАННОЕ пред-существующее
ограничение (не в объёме этого окна: авто-cleanup для деструктурированных
элементов — отдельный followup, если запросят; ручной `.close()`/
`consume X { … }` работает штатно).

## Три пробы из брифа — результаты дословно

1. **Owned-биндинги появились.**
   `consume (a, b) = p378_make_pair()` — оба `a`/`b` несут полноценное
   consume-обязательство. Забыл `b` → `[D133-not-consumed] переменная
   `b` (тип ``) не consumed до scope-exit` — диагностика ИМЕННО по `b`,
   не по всему биндингу (см. `spec_tests/conformance/neg/
   p378_consume_destructure_forget_element_neg.nv`, EXPECT_COMPILE_ERROR
   подтверждён через `test-build`).

2. **Передача элемента в consume-параметр в обычном контексте работает.**
   `consume (r, w) = p378_make_pair(); p378_pump(r, w)` (где
   `p378_pump(consume r P378Half1, consume w P378Half2)`) — `nova check`
   ok, `nova-codegen test-build` PASS (собрано и слинковано, рантайм
   прошёл — `p378_consume_destructure_ok.nv`, тест «pump(r, w)»).

3. **`E_CONSUME_BLOCK_NOT_OWNED` НЕ ослабла — проверено прямым пробоем.**
   Отдельная (не-conformance, scratch) проба: после
   `consume (r, w) = p378_make_pair()`, блок `consume r { … }` (с
   `P378Half1 consume @cleanup(...)` реализованным) — ОШИБКИ НЕ дал: `r`
   распознан как честный owned-биндинг (`ctx.consume_obligations`), тот
   же путь, что для `consume r = expr`. `nova check` ok, codegen PASS.
   Сам guard-блок остаётся ро-вью внутри своего тела (`E_CONSUME_BLOCK_
   MOVE_OUT` на попытке `return r`/присвоить наружу) — это НЕ чинилось и
   чиниться не должно (штатное поведение D188-блока, не связано с №378).

## Вердикты фикстур

`spec_tests/conformance/p378_consume_destructure_ok.nv` (module
`spec_tests.conformance`, 6 test-блоков):
- `consume (a, b) = p378_make_pair()` — positional tuple, оба элемента
  `.close()` — **PASS** (check + standalone codegen test-build).
- `consume (r, w) = p378_make_pair(); p378_pump(r, w)` — **PASS**.
- `consume {x, y} = p378_make_named_pair()` — named tuple (`type P378Pair
  consume(x T1, y T2)`), полный field list — **PASS**.
- `consume {x, y, ..} = p378_make_triple()` — партиал с explicit `..`
  (D411) — **PASS**.
- `consume x = P378Simple { id: 9 }` — регресс простого идента —
  **PASS** (байт-в-байт прежнее поведение).
- Весь `spec_tests/conformance`-каталог как ОДИН CU (module-модель
  авто-мержит все файлы папки): `nova check spec_tests/conformance/...`
  → **PASS: 1 FAIL: 0** (65 pre-existing warnings, ноль новых ошибок).
  Мега-CU codegen/test-build (интеграция ВСЕХ test-блоков папки,
  долгий прогон) НЕ гонялся — по брифу, авторитетный гейт у интегратора;
  логика подтверждена standalone-эквивалентами в изолированном scratch-
  модуле ДО перекладки в conformance (тот же код, тот же результат).

`spec_tests/conformance/neg/p378_consume_destructure_forget_element_neg.nv`
(module `neg.p378_consume_destructure_forget_element_neg`,
`EXPECT_COMPILE_ERROR D133-not-consumed`) — **PASS (negative)**, и
`nova check` напрямую подтверждает текст диагностики (по имени `b`).

`spec_tests/conformance/neg/p378_consume_destructure_positional_on_named_neg.nv`
(module `neg.p378_consume_destructure_positional_on_named_neg`,
`EXPECT_COMPILE_ERROR E_NAMED_TUPLE_POSITIONAL_DESTRUCTURE`) — **PASS
(negative)**, диагностика идентична существующей №145-форме для `ro`/`mut`.

`nova lint` на всех трёх новых файлах — **0 findings**.

## Гейты

- `cargo build` (compiler-codegen, nova-cli --release) — чисто.
- `nova check std/src` — **148 PASS / 26 FAIL / 61 WARN**, δ0 (тот же
  канон, что зафиксирован в предыдущих закрытиях этого worktree).
- arch-ratchet — **lines=64416 (<=64416), infer=348 (<=348)** — δ0,
  подтверждено `scripts/guards/arch-ratchet.sh`.
- `nova test std/src/net` — **CC-FAIL в `addr`/`split_test`**, но это
  ПРЕД-СУЩЕСТВУЮЩИЙ дефект (реестр 221.1 №166/№19/№257 —
  `NovaRes_..._IoError*` vs `nova_unit`, transitive mono
  `std.io.write_all[TcpStream]`), НЕ связан с этим окном:
  - `git status` worktree'а на момент прогона показывал изменения
    ТОЛЬКО в `compiler-codegen/src/parser/mod.rs` (гейт-правка) — ни
    один файл `std/src/net/**` не тронут.
  - Байт-в-байт воспроизведено на НЕПАТЧЕННОМ `main` той же исходной
    commit-точки (`d:\Sources\nv-lang\nova`, свежесобранный release-
    бинарь) — тот же класс ошибки (`Nova_AtomicInt`/`nova_atomic_int`
    несовпадение типов на параллельно идущей волне в main; конкретный
    текст ошибки дрейфует между прогонами из-за конкурентной работы
    в main, но CC-FAIL воспроизводится независимо от моей ветки).
  - `into_split`-специфичные фикстуры (`split_test.nv`) страдают ТОЛЬКО
    потому, что живут в том же folder-CU `std.net`, что и сломанный
    `addr.nv` — сама by-half tuple-деструктуризация (`mut (cli_rd,
    cli_wr) = conn.into_split()`, уже существующая ДО этого окна форма)
    не менялась и не является источником CC-FAIL.
  - Новый вывод/маркер не заводился — дефект уже зарегистрирован.
- `nova check spec_tests/conformance/...` (весь CU) — **PASS: 1 FAIL: 0**.
- Мега-CU / флагман — не гонялись (интегратор), по брифу.

## Спека

- `spec/decisions/05-memory.md`, раздел D180 «Что отложено (honest
  defer)»: пункт про destructuring patterns УБРАН (закрыт).
- Туда же добавлен новый амендмент «№378, `[M-73.1-destructure]`, окно
  p378-consume-destructure, 2026-08-06» — документирует обе формы,
  таблицу форм/семантики/поведения-на-именованном-кортеже, per-элементную
  линейность, канал реализации (единственная строка парсера + три
  переиспользованных механизма) и явно фиксирует, что мульти-var
  `spawn consume a, b { … }` для plan 249 НЕ покрыт (отдельная работа).
- `spec/decisions/02-types.md`, D222-амендмент 2026-08-05: добавлено
  примечание, что проверка `check_positional_destructure_on_named_tuple`
  биндинг-агностична (`let`/`ro`/`mut`/`consume` одинаково), и что
  `consume` получил ДОСТУП к деструктуризации только этим окном — правило
  само не менялось.

## Реестр

- `docs/plans/221.1-bug-sweep.md`, строка №378 — статус переведён
  🟡 открыт → ✅ ЗАКРЫТ с формулой закрытия.
- `docs/plans/backlog-followups.md`, `[M-73.1-destructure]` — статус
  переведён OPEN → ЗАКРЫТ с полной формулой закрытия (прежний диагноз
  сохранён под ней для истории).

## Модель

sonnet, окно p378-consume-destructure, без суб-агентов, без фоновых
команд (два случайных auto-background из-за 120с/180с default-таймаута
на мега-CU-подобных прогонах — оба остановлены `TaskStop` не дожидаясь
завершения, т.к. это была мега-CU-территория интегратора, не моя).
