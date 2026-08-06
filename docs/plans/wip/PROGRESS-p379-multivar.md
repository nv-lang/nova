# №379 — мульти-var `spawn consume a, b { … }` / `detach consume a, b { … }`

Модель: sonnet. Ветка `p379-multivar-spawn-consume`, worktree
`d:/Sources/nv-lang/nova-p379`.

## Что сделано

### 1. Парсер (`compiler-codegen/src/parser/mod.rs`)

`parse_spawn` (было: строго один идент через `self.parse_ident()`) и
`parse_detach` теперь после первого идента проверяют `,` и, если он есть,
уходят в новую функцию `parse_spawn_detach_consume_multivar` (добавлена
после `parse_detach`, перед `parse_blocking`). Она:

- собирает список идентов через запятую (без trailing-comma, как у
  D188-мультивар `parse_multi_reconsume_scope`);
- если после списка идёт `=` — парс-ошибка
  `E_SPAWN_CONSUME_MULTIVAR_BINDING_MIX` (смешение со связывающей формой
  запрещено, симметрично D188-мультивар);
- если после списка нет `{` — парс-ошибка с объяснением (та же формулировка
  по духу, что у single-var формы);
- строит тело: вложенные `Stmt::ConsumeScope { re_consume: false, init:
  Ident(name), .. }`, по одному слою на биндинг, самый внутренний слой —
  настоящее тело пользователя (та же форма, что `parse_multi_reconsume_
  scope` строит для `consume A, B, C { … }`, но КАЖДЫЙ слой с
  `re_consume: false`, а не `true` — это и есть разница «реальное владение»
  vs «RO-view»);
- возвращает `(Block, Span)` — вызывающая сторона (`parse_spawn`/
  `parse_detach`) оборачивает в `ExprKind::Spawn(Box<Expr::Block>)` /
  `ExprKind::Detach(Block)` соответственно (та же асимметрия обёртки, что у
  single-var форм — `Spawn` хочет `Box<Expr>`, `Detach` — `Block` напрямую).

### 2. Почему НЕ вложением (multi-move семантика)

D188-мультивар (`consume A, B, C { body }`) — чистый сахар над вложенным
re-consume в ОДНОМ И ТОМ ЖЕ владеющем scope: тело получает RO-view на
каждый идент (`E_CONSUME_BLOCK_MOVE_OUT` запрещает вынос владения изнутри).
Наша форма — противоположность: КАЖДЫЙ перечисленный биндинг должен перейти
в РЕАЛЬНОЕ владение ребёнка, move-out изнутри тела обязан быть разрешён (как
у single-var `spawn consume c { … }`).

Ключевое наблюдение (проверено по коду, не предположение): переход владения
в `spawn`/`detach` определяется НЕ порядком вложенности статементов, а
checker'овским free-variable capture-scan'ом (`capture_scan_stmt`'s
`Stmt::ConsumeScope`-ветка, `types/mod.rs`, строки ~30490): он сканирует ВСЁ
тело closure'а на owned/linear-ссылки НЕЗАВИСИМО ОТ ГЛУБИНЫ ВЛОЖЕННОСТИ и
освобождает от захвата `init`-идент КАЖДОГО слоя (`shadow.insert(binding)`,
рекурсия в `body`). Поэтому нанизанные `ConsumeScope`-слои для `a` и `b`
оба захватываются (move'ятся) в ребёнка в ОДНОЙ точке — сам
`spawn`/`detach`-statement, до входа в тело ребёнка. Вложенность —
parse-time сахар, не история последовательного move'а в рантайме. Это и
разрешило переиспользовать вложенную-`ConsumeScope`-форму, не изобретая
новый AST-узел/новую runtime-примитиву.

### 3. Чекер-канал: найденный и закрытый gap (`compiler-codegen/src/types/mod.rs`)

При обобщении вскрылось, что существующий пост-обход для `spawn`
(`ExprKind::Spawn`-ветка `consume_walk_expr`) матчил РОВНО один слой:

```rust
if let [Stmt::ConsumeScope { init, re_consume: false, .. }] = b.stmts.as_slice() {
    // mark_consumed_bypass_guard только для ЭТОГО единственного биндинга
}
```

Это нужно, потому что `consume_walk_isolated_expr` (обход тела spawn) в
конце ВОССТАНАВЛИВАЕТ `ctx.states` (снапшот до захода в тело) — без
пост-обхода внешний биндинг оставался бы «живым» в родительском scope
несмотря на реальный move. Для 2+-биндингов такой односложный матч ловил
ТОЛЬКО первый (самый внешний) идент — второй и далее оставались бы
доступны после `spawn`-statement'а (use-after-consume пролезал бы молча).

Фикс: две новые свободные функции рядом с `consume_walk_isolated_block`/
`consume_walk_isolated_expr` (~строка 41312 до правки):

- `collect_spawn_detach_reuse_names(b: &Block, out: &mut Vec<(String, Span)>)`
  — рекурсивно спускается по цепочке «Block с ровно одним
  `Stmt::ConsumeScope{re_consume:false, init:Ident(n), ..}`», собирая ВСЕ
  имена вдоль цепочки (не только первое);
- `reapply_spawn_detach_consume_moves(ctx, b)` — вызывает сборщик и для
  каждого имени, если оно реально в `ctx.consume_obligations`, зовёт
  `mark_consumed_bypass_guard`.

**Побочная находка — уже существовавший баг у `detach` (не только новый
многослойный случай):** у `ExprKind::Detach` вообще НЕ было аналога этого
пост-обхода (только у `Spawn`). Проверено живым пробоем ДО правки:

```nova
fn use_after_move() Detach -> int {
    consume j = Job.new(1)
    detach consume j { ro _p = j.payload }
    j.payload
}
```

компилировалось `ok:` — use-after-consume для «already-bound» формы
`detach consume c { … }` (без `= expr`) не ловился ВООБЩЕ, хотя симметричная
`spawn`-форма ловит его корректно (D131). Спека (D415 §4, амендмент
2026-07-22) утверждала обратное («все три [capture-check/codegen/consume-
tracker] уже были написаны generic... не потребовали отдельных изменений»)
— утверждение было неточным именно для consume-tracker'а. Исправлено тем же
слиянием (обе ветки, `Spawn` и `Detach`, теперь зовут
`reapply_spawn_detach_consume_moves`); коррекция зафиксирована в спеке
рядом со старым абзацем (не переписывая его — добавлен явный «Коррекция
(№379, 2026-08-06)» блок).

Проверено ДО и ПОСЛЕ фикса (живые пробои, не гипотеза):
- `spawn consume c { .. }; c.foo()` — ловилось И ДО фикса (D131).
- `detach consume c { .. }; c.foo()` — НЕ ловилось до фикса, ловится после.
- `spawn consume a, b { .. }; b.foo()` — НЕ ловилось (b — второй биндинг) до
  фикса, ловится после.
- `detach consume a, b, c { .. }; a.foo()` — то же для detach, 3 биндинга,
  первый.

### 4. Спека (`spec/decisions/06-concurrency.md`, D415 §4)

Добавлен амендмент «№379 (владелец, 2026-08-06): мульти-var форма» —
синтаксис, отличие от D188-мультивар (RO-view vs реальное владение),
десугар, момент move'а (capture-scan, не порядок вложенности),
**cleanup-порядок LIFO** (последний перечисленный — самый внутренний слой →
его cleanup срабатывает первым на обратном пути, как у D188-мультивар, но
здесь явно зафиксировано, а не просто «наблюдаемо через вложенность»),
use-after-consume (D131), ссылка на чекер-фикс и на мотивирующий
relay-носитель. Плюс отдельный блок «Коррекция (№379, 2026-08-06)» рядом со
старым амендментом 2026-07-22 (см. п.3 выше).

`spec/*.md` (обзорные) НЕ тронуты осознанно — `NOVA_OVERVIEW_NA=1` при
коммите: у single-var предка этой формы тоже нет упоминания в
overview.md/syntax.md (проверено grep — ноль совпадений), это деталь
D-блока, не язык-уровня обзор.

## Фикстуры (`spec_tests/conformance/`)

- **pos** `spawn_detach_consume_multivar_ok.nv` (module
  `spec_tests.conformance`, 7 `test`-блоков):
  - `spawn consume a, b { … }` — два consume-типа, оба потреблены в теле;
  - `detach consume a, b { … }` — симметрично;
  - `spawn consume a, b, c { … }` — три биндинга (список, не пара);
  - bidirectional relay: `spawn consume ar, bw { pump(ar, bw) }` /
    `spawn consume br, aw { pump(br, aw) }` на ЧЕТЫРЁХ независимых
    consume-локалах (без деструктуризации — №378 идёт параллельно, эквивалент
    построен буквально по инструкции брифа);
  - pos-регресс: одиночные `spawn consume x { … }` / `detach consume x { … }`.
- **neg** `neg/spawn_consume_multivar_use_after_neg.nv` — обращение ко
  ВТОРОМУ перечисленному биндингу (`mv379ua_b`) после `spawn`-statement'а →
  `EXPECT_COMPILE_ERROR D131` (именно тот код, который реально эмитит
  компилятор — «использование потреблённой переменной»; у single-var
  precedent'а в этом же каталоге, `neg/use_after_spawn_consume.nv`, маркер
  «undefined identifier», но там тестируется СВЯЗЫВАЮЩАЯ форма с `=`, где
  идент физически не существует снаружи блока — у нашей формы возможна
  ТОЛЬКО already-bound форма (список без `=`), там диагностика — D131,
  проверено дословно живым прогоном).
- **neg** `neg/spawn_consume_multivar_binding_mix_neg.nv` — `spawn consume
  a, b = expr { … }` → `EXPECT_COMPILE_ERROR E_SPAWN_CONSUME_MULTIVAR_
  BINDING_MIX`.

Workflow: все три фикстуры сначала прогнаны ИЗОЛИРОВАННО (отдельные
временные директории/модули в scratchpad, не в общем
`spec_tests.conformance`, по протоколу test-conventions.md «Workflow
добавления D-теста») — дословные вердикты см. ниже — и только после PASS
скопированы в реальные пути с доменным префиксом типов (`Mv379*`/`mv379_*`)
во избежание коллизий имён в общем CU (проверено grep — коллизий с
существующими файлами `spec_tests/conformance/*.nv` нет).

### Вердикты фикстур (живые прогоны, debug-биналь до финального release):

```
spawn_detach_consume_multivar_ok.nv (изолированный прогон, module
p379_iso_pos эквивалент содержимого):
  ok: ... — PASS: 1 FAIL: 0

neg/spawn_consume_multivar_use_after_neg.nv (реальный путь в репе):
  FAIL: ...:30:5: error: использование потреблённой переменной `mv379ua_b`
  (D131): её значение отдано consume-вызовом и больше недоступно
  note: значение потреблено здесь --> ...:28:34
  → совпадает с EXPECT_COMPILE_ERROR D131

neg/spawn_consume_multivar_binding_mix_neg.nv (реальный путь в репе):
  FAIL: ...:20:46: error: [E_SPAWN_CONSUME_MULTIVAR_BINDING_MIX] `spawn
  consume a, b = expr { … }` is not valid (№379): ...
  → совпадает с EXPECT_COMPILE_ERROR E_SPAWN_CONSUME_MULTIVAR_BINDING_MIX
```

Дополнительные живые пробои (не коммичены — только для верификации
семантики в процессе разработки):
- 3-биндинговый `detach consume a, b, c { pump3(a, b, c) }` — компилируется
  чисто; `a.payload` после statement'а → D131 (первый биндинг, самый
  внешний слой — подтверждает, что фикс покрывает не только «последний»/
  «второй», а ЛЮБОЙ индекс в цепочке).

## Разблокировался ли relay (мотивирующий носитель)

Частично — по формулировке брифа: «эквивалент на паре consume-локалов без
деструктуризации» (№378 деструктуризация `consume (ar, aw) = …` идёт
параллельно, не входит в это окно). Построенный в pos-фикстуре
bidirectional-relay-тест (4 независимых consume-локала `mv379_ar/aw/br/bw`,
два `spawn consume … , … { pump2(…, …) }`) компилируется и корректно
типизируется — это и есть конкретное доказательство, что мульти-var форма
снимает блокер «файберу нельзя отдать обе половинки». Полный носитель
(с реальным `TcpStream.into_split()` + деструктуризацией) требует №378
(параллельное окно) — вне объёма этого окна.

## Гейты

- `cargo build` (compiler-codegen, debug и nova-cli debug+release) — чисто,
  без новых warning'ов от правки (были только уже существовавшие dead-code
  warning'и).
- `arch-ratchet`: БЕЗ ИЗМЕНЕНИЙ — `lines=64416 <= 64416`, `infer=348 <= 348`
  (правка не трогает `emit_c.rs` вообще, только парсер + чекер-канал).
- `nova check std/src` (release-биналь): `PASS: 148 FAIL: 26 WARN: 61` —
  ВСЕ 26 FAIL — уже существующие `neg/`-фикстуры (encoding/serde_neg,
  fs/neg, io/neg, net/neg, time/civil/neg — `nova check` не распознаёт
  `EXPECT_COMPILE_ERROR`-маркеры, это ожидаемо и не связано с правкой).
  Список файлов сверен построчно — ни один не относится к
  spawn/detach/consume/concurrency.
- `nova test std` (release-биналь): полный прогон падает на ДВУХ
  environment-специфичных pre-existing проблемах, ПОДТВЕРЖДЁННЫХ байт-в-
  байт на чистом `main` (тот же release-биналь main-репы, без моих правок):
  1. `CC-FAIL std/src/concurrency/retry_test` — тот же файл/строка/текст
     ошибки (`incompatible operand types 'nova_int' and 'nova_unit'`) на
     main и на worktree.
  2. `nova: internal error ... [P67-LEGACY] Path call return type unknown
     for method=now` — тот же класс ICE на main (строка в `emit_c.rs`
     отличается только из-за смещения кода правкой, сообщение и причина
     идентичны).
  3. `CC-FAIL std/src/net/addr` — тот же файл/строки/текст ошибки на main и
     worktree (проверено `--filter net` на обеих сторонах).
  Поскольку раннер (`jobs=16`, недетерминированный порядок) падает целиком
  при первом ICE, получить полный PASS/FAIL-дельту по ВСЕМУ `std` в этом
  окружении невозможно ни для main, ни для ветки — это ограничение
  окружения, не регрессия правки. Точечно прогнаны фильтром модули,
  реально использующие `spawn`/`detach consume` в std
  (`--filter runtime/sync`, `--filter supervisor`) — оба:
  `PASS: 1 FAIL: 0` на ветке, чисто.
- Мега-CU `spec_tests/conformance` — НЕ гонял (по прямому указанию брифа,
  гейт у интегратора).
- Флагман-examples — НЕ гонял (гейт у интегратора).

## Модель

sonnet (вся волна — разведка, парсер, чекер-фикс, фикстуры, спека,
верификация гейтов).
