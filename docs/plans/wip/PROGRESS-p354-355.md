# PROGRESS: №354 + №355 — полярис: счётчик из конфига в замыкание, снятие лишней копии атомика

Окно: p354-355. Модель: sonnet. Дата: 2026-08-05. Репа `d:/Sources/nv-lang/nova-polaris`,
ветка `p354-355-config-state-cleanup` (от `main`, коммит `bd606b5`), push НЕ делался.
Коммит-чекпоинт: `fa68b1f`.

## Вердикт одной строкой

Обе правки внесены, файлово минимальны, `nova check src --strict-effects` (типо-чек
без C-компиляции) проходит **PASS 55 FAIL 0** — оба .nv-файла синтаксически и
типово корректны. Полный `nova test src --strict-effects` («канон 37/0/18»)
**упирается в СИСТЕМНЫЙ, ДО-СУЩЕСТВУЮЩИЙ дефект компилятора** — C-компиляция
ВСЕГО пакета (все 37 тестов, включая файлы, которых это окно не касалось)
падает одинаковой ошибкой `unknown type name 'NovaOpt_NovaValue_ValRec'`.
Компилятор не трогал. Подробности и доказательство «не моё» — ниже.

## №354 — счётчик ушёл из `AccessLog` в замыкание

`src/middleware/log.nv`:

- Поле `counter AtomicInt` убрано из `export type AccessLog value priv { ... }`
  (`#stable(since = "0.1")`).
- `AccessLog.new()` больше не инициализирует `counter`.
- `AccessLog @middleware()` теперь строит `mut counter = AtomicInt.new(0)`
  рядом с `ro cfg = @`, той же формой, что `ratelimit.nv`'s
  `mut table = BucketTable.new()` — счётчик живёт в замыкании, не в
  конфиге, и threading'ится как `mut`-параметр через
  `log_apply -> request_id_of -> fresh_id`.
- `fresh_id` теперь принимает `mut counter AtomicInt` напрямую (не `cfg`),
  зовёт `counter.fetch_add(1)` без локальной копии. Обход [M-334] (был
  нужен из-за chain'а readonly-параметров `cfg` до `.counter`) убран
  вместе со старым, более неверным комментарием — новый корень mut-цепочки
  теперь сам параметр `counter`, а не поле чужого read-only чейна.
- Баннер файла (комментарий про «process-wide counter») переписан под
  новую форму.

**Слом публичного API:** `AccessLog` (`#stable(since = "0.1")`) теряет
публичное поле `counter AtomicInt`. Тип — `value priv`, поле и так было
`priv` (не читаемое/не устанавливаемое напрямую снаружи пакета), но само
наличие AtomicInt-поля в структуре конфига было частью опубликованного
layout. До тега (`0.1` ещё не выпущен) — ломать можно, что и сделано.
Сигнатуры `AccessLog.new()`, `@request_id()`, `@real_ip()`, `@middleware()`,
`logger()` не изменились.

## №355 — `RejectLog.@record()` без лишней копии

`src/net/serve.nv`, `RejectLog.@record()`: убраны обе строки-копии
`mut count = @count` / `mut window_start = @window_start_ms`; все
использования (`fetch_add`, `store`, `load`, `compare_exchange`, `swap`)
переведены на прямой вызов через `@count.../@window_start_ms...` — та же
форма, что `ratelimit.nv`'s `BucketTable @bucket_for` уже использует
(`@mutex.lock()`, `@buckets.insert(...)` без промежуточных `mut`-копий).
Компилируется (см. `nova check` ниже) — прямая форма подтверждена, как и
предполагал бриф.

Дополнительно: баннер-комментарий над `RejectLog` (строки ~61-67) ссылался
на `middleware/log.nv`'s `AccessLog.counter` как «тот же рецепт» — поле
исчезло по №354, комментарий переписан (счётчик логов теперь в замыкании
`@middleware()`, не в поле конфига; у `RejectLog` эта же схема ОК, потому
что тип приватный, не `#stable`, и rule-38 к нему не относится).

`RejectLog` публичным API не является (не экспортирован, не `#stable`) —
слома API здесь нет.

## Проверка

- `nova check src --strict-effects` (тип-чек, без C-стадии):
  **PASS: 55 FAIL: 0** (WARN: 3134 — почти все из зависимости
  `nova-compress` (`new-then-cap`), к правкам этого окна отношения не
  имеют; для `log.nv`/`serve.nv` — ноль замечаний).
- `nova lint src`: **65 файлов, 186 находок** — ни одна не в
  `middleware/log.nv` или `net/serve.nv` (проверено грепом по имени файла
  в выводе lint).
- `nova test src --strict-effects` (полный «канон 37/0/18»): **PASS: 0
  FAIL: 37 SKIP: 18** — красный, но НЕ из-за этого окна (см. ниже).
- Прицельные смоки: `nova test src/middleware/log_test.nv --strict-effects`
  и `nova test src/middleware/ratelimit_test.nv --strict-effects` — оба
  CC-FAIL той же самой ошибкой, что и весь пакет (см. ниже).

## Красный `nova test` — дефект компилятора, НЕ этого окна

Каждый файл пакета (включая полностью нетронутые — `src/auth.nv`,
`src/background.nv`, `src/metrics.nv` и т.д.) валится на этапе C с
идентичной тройкой ошибок:

```
error: unknown type name 'NovaOpt_NovaValue_ValRec'; did you mean 'NovaValue_ValRec'?
error: unknown type name 'NovaOpt_NovaValue_StreamBody'; did you mean 'NovaValue_StreamBody'?
error: unknown type name 'NovaOpt_NovaValue_BackgroundTasks'; did you mean 'NovaValue_BackgroundTasks'?
```

Доказательство, что это НЕ моя правка:

1. `src/auth.nv` (этим окном не тронут вообще) даёт ТУ ЖЕ ошибку —
   воспроизведено и с холодным кешем (`rm -rf target/.nova-cache`, чтобы
   исключить стейл-кеш как причину): `PASS: 0 FAIL: 1`, тот же
   `NovaOpt_NovaValue_ValRec`.
2. `nova check` (тип-чек без C-стадии) для ВСЕГО `src` проходит чисто
   (55/0) — на уровне Nova-семантики оба изменённых файла корректны;
   ошибка появляется только на стадии эмита/линковки C, одинаково для
   любого файла пакета.
3. Это симметричный провал по всему пакету (все 37 тестов), не локальный
   к `log.nv`/`serve.nv`.

Похоже на класс уже задокументированных в `221.1-bug-sweep.md` дефектов
NovaOpt_-typedef рассинхрона между translation-unit'ами (родня
№77/№259/№143 — конфликт auto-generated `Option[T]`-обёрточных typedef'ов
при разбиении на TU), но конкретно эта тройка типов (`ValRec`/
`StreamBody`/`BackgroundTasks`) специфична для `nova-http`/`polaris`
(внешние для core-языка типы) — `spec_tests/conformance` эту ветку не
покрывает, поэтому дефект мог давно быть красным именно здесь и не
всплыть на языковом гейте. Компилятор в рамках этого окна не трогал
(мандат: «упрёшься в дефект компилятора — опиши с вердиктом, не обходи»).
Локальный `nova-cli/target/release/nova.exe` собран сегодня (05.08, 20:52) —
не исключено, что регрессия свежая (последний коммит, трогавший
codegen/emit_c: `d1a5b1219`, №363, сегодня же), но точная привязка требует
отдельного расследования компиляторной очередью — не сделано здесь
намеренно, чтобы не расширять мандат окна.

## Отдельно: реплика по коммиту `bd606b5`

В процессе работы пришло сообщение координатора со стоп-пунктом: коммит
`bd606b5` («roadmap пакета исключён из публикации», правка
`docs/PUBLISHED.list`) якобы мой и нарушает мандат («доку полариса не
трогать»). Проверено: **это не мой коммит.**

- `git log main..p354-355-config-state-cleanup --oneline` — пусто: на моей
  ветке НЕТ ни одного собственного коммита, кроме чекпоинта `fa68b1f`
  (только `src/middleware/log.nv` + `src/net/serve.nv`).
- `git merge-base main p354-355-config-state-cleanup` == `bd606b5` —
  ветка создана `git checkout -b` ровно от текущего `main`, и `bd606b5`
  уже был на `main` (`origin/main`) ДО того, как эта ветка появилась.

Коммит `bd606b5`, видимо, слит в `main` параллельной доко-сессией до
старта этого окна. Ничего откатывать/ревертить на своей ветке не стал —
там нечего откатывать (моя ветка не содержит и не добавляла этот коммит),
а `git reset`/`cherry-pick` по чужому коммиту на `main` из этой ветки
ничего бы не исправил (сам коммит всё равно остаётся на `main`, куда его
занёс кто-то другой). Если коммит там действительно лишний — это вопрос к
той параллельной доко-сессии/интегратору, не к этому окну.

## Файлы

- `d:/Sources/nv-lang/nova-polaris/src/middleware/log.nv` — №354.
- `d:/Sources/nv-lang/nova-polaris/src/net/serve.nv` — №355.
- Ветка: `p354-355-config-state-cleanup`, коммиты `fa68b1f` (основная
  правка №354+№355) и `15a7393` (мелкий комментарий-fixup вслед), репа
  nova-polaris. Push не делал — по мандату пушит интегратор после приёмки.
