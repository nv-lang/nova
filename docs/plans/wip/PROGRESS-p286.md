# PROGRESS — окно p286-chan-type (№286 + №143, тип элемента канала)

Модель: sonnet. Worktree: `d:/Sources/nv-lang/nova-p286`, ветка `p286-chan-type` от `main` (c021d6ab1).
Ветку НЕ вливать и не пушить — по правилам задания.

## Главная находка: бриф оказался ЧАСТИЧНО устаревшим

№286 и №143 в `docs/plans/221.1-bug-sweep.md` **УЖЕ помечены ✅ ЗАКРЫТ 2026-08-02** (окно
`p-chan`, коммиты `dabe9b184`/`6e7db5d2c`, **уже влитые в main** ДО того, как ветвился этот
worktree — HEAD `main` в момент `git worktree add` был `c021d6ab1` от 2026-08-04, на два дня
позже влития p-chan). Проверено чтением `git log --oneline -- PROGRESS-pchan.md` в самом
worktree и содержимым `compiler-codegen/src/types/mod.rs` — фикс реально там, не в архиве.

Это НЕ означает «задание неактуально»: бриф явно предупреждал «половина канала уже
проложена — посмотри, что именно там сделано, прежде чем строить своё». Проверка того, что
там сделано, и есть первая половина этого окна. **Вторая половина — реальная, найденная и
починенная дыра, оставшаяся ПОСЛЕ p-chan (см. ниже).**

## Что уже было сделано окном p-chan (проверено, не переделывалось)

`compiler-codegen/src/types/mod.rs` (чекер-канал, §0/196, НЕ emit_c.rs):
- `channel_new_turbofish_elem` — вытаскивает явный `T` из `Channel[T].new(...)`.
- `f1_stmt`'s Tuple/Record destructure-ветки — `ro (tx, rx)` / `ro { tx, rx }` от
  `Channel[T].new` получают `scope["tx"] = ChanWriter[T]`, `scope["rx"] = ChanReader[T]`.
- `channel_elem_type` — читает T обратно из scope для `.recv()`/`.send()`.
- `.recv()`/`.try_recv()` типизируются `Option[T]`; `.send()`/`.try_send()` проверяют
  аргумент против `T` — новая диагностика `E_CHANNEL_ELEM_TYPE_MISMATCH`.
- Симметрично для параметров/локалов, аннотированных `ChanReader[T]`/`ChanWriter[T]`
  напрямую (уже работало бесплатно, поскольку тип параметра идёт из `p.ty`).

**Проверено этим окном (не переделано, а перепроверено на СВЕЖЕМ билде из этого
worktree'а):**
- `cargo build --release` (nova-cli) — чисто.
- `nova check std/src` — **148/26/61**, байт-в-байт канон.
- polaris `--strict-effects` (`src`) — **37/0/18**, байт-в-байт канон.
- `arch-ratchet` — `lines=64542<=64545`, `infer=348<=348` (эта строка не менялась мной на
  этом этапе — фикс p-chan и мой фикс оба живут ТОЛЬКО в `types/mod.rs`, `emit_c.rs` не
  тронут никем).
- **Обязательное требование брифа — «проверь на ОБЪЕДИНЁННОМ CU, не только на одиночной
  фикстуре»**: собрал изолированный repro из `pos_channel_send_consume_share.nv` (turbofish
  `Channel[Vec[int]]`, `.len()`) + `pos_channel_elem_type_record_sum_map.nv` (turbofish
  `Channel[HashMap[str,int]]`, `.len()`) + `d412d_embed_dir.nv` (`EmbeddedDir.len()` —
  ИМЕННО тот коллизионный партнёр, на котором ORIGINAL №286 был найден s1b-окном) в одной
  папке (все три файла — `module spec_tests.conformance`, поэтому компилятор трактует их
  как ОДИН compile-unit — ровно механизм мега-CU, только маленький и контролируемый).
  Результат: `Running 5 tests... 5/5 passed` — ТРИ разных типа с методом `.len()`
  (`EmbeddedDir`, `HashMap`, `Vec[int]`) сосуществуют в одном CU, каждый резолвится
  ПРАВИЛЬНО. До фикса p-chan это было ровно то сочетание, что давало недетерминированный
  результат. **Вывод: ядро №286 (turbofish/annotated случай) закрыто по-настоящему, не
  только «зелёные вердикты» — проверено измерением, а не пересказом.**

## Находка этого окна: №286 residual gap для BARE `Channel.new`

p-chan **осознанно и честно** не тронул случай `ro (tx, rx) = Channel.new(cap)` БЕЗ
турбофиша/аннотации — `docs/guide/channels.md` был переписан на прямой текст: обещание
«T inferred from first send/recv» никогда реально не выполнялось, и это НЕ будет починено
этим окном (design note в `PROGRESS-pchan.md`, раздел «ЧТО НЕ СДЕЛАНО»).

**Измерил, что эта «недокументированная permissiveness» — не безобидна.** Репро
(`docs/plans/repros/p286-bare-channel-erased/bare_len_probe.nv`, коммит `840ea8576`,
RED без фикса — проверено на бинаре ДО правки):

```nova
ro d = embed_dir("bare_dir")          // EmbeddedDir, .len() == 1 (1 файл)
ro (tx, rx) = Channel.new(2)          // T НЕ отслеживается (бриф p-chan)
mut v []int = []int.new()
v.push(10); v.push(20); v.push(30)
tx.send(v)
ro got = rx.recv()
match got {
    Some(vv) => { assert(vv.len() == 3) }   // ждём Vec.len()==3
    ...
}
```

**Компилировалось без единой ошибки чекера и падало на assert** (`vv.len() == 3` даёт
`false`) — `.len()` молча резолвился в `Nova_EmbeddedDir_method_len` (через легаси
name-only `method_receivers`-фоллбэк в `emit_c.rs`), а не в `Vec.len()`. Это ровно
определение К1 — компилируется, запускается, молча делает не то. Прежде это НЕ было
измерено — только предсказано в тексте маркера №286 («требует полноценной генерик-
монофикации ЛИБО минимум сделать name-only fallback громким»); теперь есть прямое
воспроизведение с конкретной провалившейся строкой.

## Фикс (место: чекер-канал, `types/mod.rs`, НЕ emit_c.rs)

Коммит `6c47cd21d`. Идея — реализовать честно то, что документация ОБЕЩАЛА и что p-chan
сознательно отложил: `T` выводится из ПЕРВОГО `.send`/`.try_send`-вызова на writer'е,
текстуально ПОЗЖЕ в ТОМ ЖЕ блоке (без рекурсии во вложенные блоки/ветки — консервативно,
покрывает обычный линейный код, ради которого и была написана исходная фраза в доке).

Механика:
1. Новое поле `channel_bare_send_elem_hint: RefCell<HashMap<ExprId, TypeRef>>` на
   `TypeCheckCtx` — тот же паттерн, что `resolved_types_buf`/`pattern_variant_types_buf`.
2. Новый метод `seed_channel_bare_send_hints(&self, b: &Block, scope)` — ОДИН проход по
   верхнеуровневым стейтментам блока: ведёт `sim_scope` (клон реального `scope`,
   прогрессивно пополняемый по мере прохода простых `let`/`ro`/`mut`-биндингов — иначе
   `mut v []int = ...` перед `tx.send(v)` не резолвился бы, потому что реальный `f1_stmt`
   ещё не дошёл до этого `let` на момент пре-паса); для каждого `Channel.new`
   tuple/record-деструктура БЕЗ турбофиша запоминает имя writer'а как «pending»; при первой
   встрече `<tx_name>.send(v)`/`.try_send(v)` инферит тип `v` через `sim_scope` и пишет
   хинт, keyed по `ExprId` самого вызова `Channel.new`.
3. `f1_block` зовёт `seed_channel_bare_send_hints` ОДИН РАЗ перед основным циклом
   `f1_stmt`.
4. Обе существующие Tuple/Record-ветки в `f1_stmt` (те же, что p-chan написал) теперь
   читают `channel_new_turbofish_elem(...).or_else(|| хинт по ExprId)` — турбофиш
   по-прежнему приоритетнее, хинт — фоллбэк.

**Строго аддитивно**: если хинта нет (нет send'а в этом же блоке, или это блок с чем-то
экзотичным вроде вложенного if/while) — поведение НЕ ОТЛИЧАЕТСЯ от до-фикса (untracked, как
раньше). `channel_elem_type`, диагностика `E_CHANNEL_ELEM_TYPE_MISMATCH`,
`resolved_types_buf`-продюсер для `recv()` — НИЧЕГО из этого не трогалось: как только
`scope["tx"]` получает конкретный `T`, вся уже существующая инфраструктура p-chan работает
на нём БЕСПЛАТНО (ровно та же причина, по которой сам p-chan не трогал `emit_c.rs` —
инфраструктура генериков уже универсальна).

**Побочный эффект (желательный, не побочный ущерб):** второй `send` НЕСОВПАДАЮЩЕГО типа
на том же writer'е в том же блоке теперь тоже ловится `E_CHANNEL_ELEM_TYPE_MISMATCH` — то
же самое гарантия, что уже была у турбофиш-формы. Проверено
(`docs/plans/repros/p286-bare-channel-erased/second_send_mismatch_probe.nv`):
`tx.send(10); tx.send("oops")` → `[E_CHANNEL_ELEM_TYPE_MISMATCH] cannot send a value of
type 'str' into a channel declared 'Channel[int]'`.

## Что вскрылось в корпусе после включения типовой проверки

Бриф прямо предупреждал: находки — это находки, не повод ослаблять. Прогнал ВЕСЬ корпус,
где встречается голый `Channel.new(` (грепом по `**/*.nv`, за вычетом `docs/plans/repros`):

- **`std/src/**`** — `nova check std/src` **148/26/61 байт-в-байт**, ни одного НОВОГО FAIL.
- **`nova-polaris/src`** — `--strict-effects` **37/0/18 байт-в-байт**, ни одного нового FAIL.
- **`bench/`, `examples/` (кроме флагмана — не мой гейт), `nova_tests/concurrency`,
  `nova_tests/negative_capability`, `nova_tests/plan83_10`** — прогнал `nova check`
  прицельно по каждому файлу, использующему `Channel.new(`:
  - `examples/flagship/aggregator/**` (не-флагманские точки входа, без полной C-сборки),
    `examples/mini_aggregator.nv`, `examples/tour/concurrency.nv`,
    `bench/m_n/handler_chain_pingpong.nv`, `nova_tests/negative_capability/*`,
    `nova_tests/plan83_10/neg/channel_negative_capacity_panic.nv` — **все чисто**, НИ ОДНОЙ
    новой ошибки.
  - `bench/corpus/05_channels_select.nv` — FAIL, но это PARSER-ошибка
    (`expected '.', got '<'` на `msg <- rx_a`, синтаксис `select` с `<-`, которого текущий
    парсер не поддерживает вовсе) — структурно не может быть вызвана моим фиксом: парсинг
    происходит ДО того, как чекер вообще запускается. Pre-existing, не регрессия.
  - `nova_tests/concurrency/neg/channel_bare_type_not_instantiable.nv` — FAIL под `nova
    check`, но это САМ ПО СЕБЕ `EXPECT_COMPILE_ERROR`-фикстура (`Channel[T]` как тип
    параметра обязан быть отвергнут `E_CHANNEL_TYPE_NOT_INSTANTIABLE`) — `nova check` не
    разбирает EXPECT-маркеры, поэтому «FAIL» здесь и есть ожидаемый успех. Не регрессия.

**Итог по корпусу: НИ ОДНОГО места, где включение типовой проверки для BARE `Channel.new`
превратило ранее проходивший код в красный.** Это отчасти объясняется тем, что реальный
код в этом репозитории почти везде либо (а) уже использует турбофиш/аннотацию (после
p-chan это стал предпочитаемый стиль, `docs/guide/channels.md` прямо советует), либо
(б) шлёт РОВНО ОДИН согласованный тип на голый канал (что и предполагает первый-send
инференс — иначе откуда бы взялась сама фраза в доке).

Флагман (`examples/flagship/aggregator` полной C-сборкой) и мега-CU (652 файла) — по
прямому слову брифа — оставлены интегратору при приёмке, сам не гонял.

## Фикстуры (что добавлено, коммит `c037037ee`)

- `spec_tests/conformance/pos_channel_bare_first_send_infer.nv` — ВЕРХНЕУРОВНЕВЫЙ файл
  (`module spec_tests.conformance`), намеренно в той же группе, что `d412d_embed_dir.nv`
  (коллизионный партнёр оригинальной находки №286) — постоянное регресс-покрытие вместо
  разового репро. Проверен вместе с `d412d_embed_dir.nv` в изолированной паре: `2/2
  passed`.
- `spec_tests/conformance/neg/channel_bare_first_send_mismatch_neg.nv` —
  `EXPECT_COMPILE_ERROR E_CHANNEL_ELEM_TYPE_MISMATCH`, второй send несовпадающего типа на
  том же голом writer'е. Прогнан индивидуально: `PASS`.
- `nova lint` на обоих — **2 files, 0 findings**.
- Существующие p-chan-фикстуры (6 neg + 5 pos + 1 panic, полный список см. `git grep -l
  channel spec_tests/conformance`) — прогнаны все вместе `--full`: **7/7 PASS** (neg+panic
  lane; pos-lane верхнеуровневые файлы не листятся по отдельности раннером — известное
  свойство "module spec_tests.conformance = один CU", не регрессия этого окна, см. ниже).

## Побочная находка о самом раннере (НЕ мой дефект, для протокола)

`nova test --list`/`--filter` не листит верхнеуровневые файлы `spec_tests/conformance/*.nv`
(`module spec_tests.conformance`, много файлов с ОДНИМ и тем же module-именем) по
отдельности — все они сливаются в ОДНУ compile-unit, а раннер показывает результат под
именем ПЕРВОГО ПО АЛФАВИТУ файла группы (напр. `d412d_embed_dir` вместо `pos_channel_...`),
хотя фактически исполняет `test`-блоки ИЗ ВСЕХ файлов группы (подтверждено `-v`:
`Running 5 tests...` для 3-файловой группы с суммарно 5 `test`-блоками, все PASS). Это
ровно тот же класс, что уже зарегистрированный №285 (`[тест-раннер]... репортит имя
файла-носителя строки CU, а не файла с упавшим assert`) — не новый маркер, находка
подтверждает существующий №285 на новом примере (`nova test --list` полностью пропускает
такие файлы, не просто путает имя при FAIL).

## Гейты — сводка (дословные строки)

- `cargo build --release` (nova-cli, из `nova-cli/`) — чисто (только pre-existing
  warnings, `dead_code`/`unused`), `Finished \`release\` profile [optimized] target(s) in
  3m 08s`.
- `nova check std/src` → `PASS: 148  FAIL: 26  WARN: 61` (байт-в-байт канон).
- polaris `nova.exe test src --strict-effects` (прямым бинарём этого worktree, env
  `NOVA_RT_DIR`/`NOVA_CG_INCLUDE`/`NOVA_STD_PATH` указаны на этот worktree, `NOVA_GC_*` —
  на главный репозиторий) → `PASS: 37  FAIL: 0` + 18 SKIP (байт-в-байт канон).
- `bash scripts/guards/arch-ratchet.sh` → `arch-ratchet ok: lines=64542 <= 64545`,
  `arch-ratchet ok: infer=348 <= 348`.
- `nova lint` на обеих новых фикстурах → `lint: 2 file(s), 0 finding(s)`.
- Свои фикстуры `nova test` (не только `check`) — см. раздел «Фикстуры» выше, строки
  прогонов приведены дословно.
- Мега-CU (652/0/68 канон) и флагман — НЕ гонял, по прямому слову брифа; интегратор при
  приёмке.

## Для интегратора

- Обе находки реестра (№286, №143) уже были ✅ ЗАКРЫТЫ окном p-chan 2026-08-02 — этим окном
  ПЕРЕПРОВЕРЕНЫ, регрессий не найдено.
- Этим окном найден и починен ОСТАТОЧНЫЙ дефект того же класса (bare `Channel.new` без
  турбофиша/аннотации) — НЕ заводил новый номер по прямому слову брифа («номера не
  присваивать — находки текстом»); реестр (`221.1-bug-sweep.md`/`backlog-followups.md`) НЕ
  правил — по конвенции файла статусы обновляет интегратор при вливании.
- `docs/guide/channels.md`/`channels.ru.md` **не обновлены этим окном** — текст «T inferred
  from first send/recv... that promise never actually held» (строки ~143-148
  `channels.md`) теперь ЧАСТИЧНО устарел: для линейного кода в том же блоке обещание ТЕПЕРЬ
  выполняется. Формулировку стоит смягчить при вливании (интегратору решать точную
  редакцию — вне зоны этого окна, чтобы не смешивать код-фикс с доке-правкой без его
  вердикта по формулировке).
- Оставшийся класс, ПОДТВЕРЖДЁННЫЙ но НЕ починенный (сознательно, вне периметра): send из
  ВЛОЖЕННОГО блока/ветки (`if`/`while`/nested block) на bare-writer — pre-pass не
  рекурсирует, поэтому такой код остаётся untracked (не РЕГРЕССИРУЕТ — просто не получает
  новую защиту). Если это станет живым риском — тот же `seed_channel_bare_send_hints`
  нужно научить спускаться в `ExprKind::Block`/`If`/`While` тел, это отдельный, чуть
  больший объём.
