# PROGRESS — окно p433-json-result-contract

Модель: Claude Sonnet 5. Worktree: `d:/Sources/nv-lang/nova-p433`, ветка `p433-json-result-contract`.

## Дефект №433 (реестр 221.1)

`std/src/encoding/json.nv:458`: `Lexer.@char_at(p int) -> char => @input[p..].chars().next()!!`.
`Json.parse`/`JsonValue.try_from` (`-> Result[JsonValue, ParseJsonError]`) обещают честный
`Err` на битом вводе, а `char_at` внутри делал `!!` — обман контракта Result. Ужесточение
№428 сделало это видимым как `[E_BANG_REQUIRES_FAIL]` и заблокировало сборку всей цепочки
`json.nv` → `serde/json.nv` → `crypto/jwt.nv` → `jwt_test.nv`.

## Фикс

`char_at` теперь `-> Option[char]` (не `char`, без `!!`). Все три вызывающих места
конвертируют `None` в `ParseJsonError.UnexpectedEof` вместо падения:

| Место | Было | Стало |
|---|---|---|
| `next_token`, ветка `BadTok` (:502) | `ro bad_char = @char_at(@pos)` (паника на `None`) | `match @char_at(@pos) { Some(c) => Ok(BadTok(c)), None => Err(UnexpectedEof{..}) }` |
| `read_keyword`, ветка mismatch (:540) | `found: @char_at(@pos)` (паника на `None`) | `match @char_at(@pos) { Some(c) => Err(UnexpectedChar{found:c}), None => Err(UnexpectedEof{..}) }` |
| `read_escape`, ветка unknown escape (:613) | `seq: "\\${@char_at(@pos-1)}"` (паника на `None`) | `match @char_at(@pos-1) { Some(c) => Err(InvalidEscape{seq}), None => Err(UnexpectedEof{..}) }` |

### Уточнение механизма (важно для отчёта дословно)

Буквальный сценарий "позиция ровно на конце ввода" в норме НЕ достижим через обычный
разбор — все три места вызова `char_at` защищены предшествующим `@peek()`/`@advance()`,
которые уже подтвердили наличие байта. Прямая проба (см. ниже, "проба на негодное")
подтвердила падение именно через синтетический прямой вызов `char_at(len)` — ровно тот
случай, что описан в тексте дефекта. Это НЕ отменяет реальность контрактного нарушения:
компиляторный `E_BANG_REQUIRES_FAIL`-чек — статический (по типам/эффектам), не по
достижимости в рантайме; это ровно то, что чек и обязан ловить (Result, обещающий Err,
не должен содержать необработанный `!!`, ДАЖЕ если сегодняшний control-flow его не
достигает — это защита от будущих изменений控制-flow, которые сделали бы его достижимым
незаметно).

## Класс: «функция, возвращающая Result, содержит бросок» — разведка по всему std/src

Инструмент: `grep -rlE '\-> Result\[' std/src --include=*.nv` (54 файла, без `_test.nv`) →
пересечение с файлами, где встречается `!!` (25 файлов).

| Файл | `!!` в коде? | Вердикт |
|---|---|---|
| `encoding/json.nv` | ДА (Lexer.@char_at) | **НАСТОЯЩИЙ носитель — почищено в этом окне** |
| `fs/fs.nv:327` (`File.@cleanup`) | ДА (`@close()!!`) | НЕ нарушение — `@cleanup` сам объявляет `Fail[IoError]` в СВОЕЙ сигнатуре (D432 §1, `-> ()`, не `Result`) — честный контракт |
| `io/buffered.nv:105` (`BufWriter.@cleanup`) | ДА (`@close()!!`) | То же — `Fail[IoError]` продекларирован, `-> ()` |
| `_experimental/crypto/insecure_demo_kdf.nv` | только в `///`-докпримере | не код |
| `data/semver.nv`, `data/semver_range.nv` | только в `///`-докпримерах | не код |
| `identifiers/snowflake.nv` | только в `///`-докпримерах | не код |
| `math/complex.nv`, `math/statistics.nv` | только в `///`-докпримерах/комментариях | не код |
| `net/addr.nv`, `net/mock.nv`, `net/udp.nv` | только в `///`-докпримерах | не код |
| `prelude.nv`, `prelude/core.nv`, `prelude/protocols.nv` | только в комментариях/докпримерах | не код |
| `runtime/read_buffer.nv` | только в комментарии | не код |
| `text/regex.nv` | только в `///`-докпримере | не код |
| `time/civil/*.nv` (date/datetime/offset/parse/period/time_of_day/tz/zoned), `time/cron.nv` | только в `///`-докпримерах | не код |
| `encoding/serde/json.nv`, `crypto/jwt.nv` | нет собственного `!!`, ошибки были ТРАНЗИТИВНЫЕ через `Json.parse` | почищено вместе с `json.nv` (те же 9 diagnostic-строк исчезли) |

**Вывод класса:** во всём `std/src` единственным настоящим носителем обмана контракта была
`Lexer.@char_at` в `json.nv`. Носителей за пределами блокирующей цепочки json+jwt для
доклада владельцу **нет** — разведка не нашла других файлов, требующих номера.

## Приёмка

### 1. `nova check std/src`

- До фикса: `PASS: 146  FAIL: 31  WARN: 54` (9 diagnostic-строк `E_BANG_REQUIRES_FAIL` —
  json.nv:894/895/908, serde/json.nv:399/411/425, jwt.nv:74/102/112, ×N дублей по разным
  compile-юнитам одного прогона).
- После фикса: **`PASS: 151  FAIL: 26  WARN: 61`** — канон восстановлен, `E_BANG_REQUIRES_FAIL`
  — 0 вхождений. Остаток 26 FAIL — не связан (`serde_neg/untagged.nv` — известный gate
  `[E_SERDE_UNTAGGED_GATED]`, `time/civil/neg/period_not_duration.nv` и
  `time_of_day_not_period.nv` — существовавшие NEG-кейсы, тоже вне темы).

### 2. Поведенческая проба (прогон, не типизация) — дословные вердикты

Standalone-бинарь (`nova build`, не мега-CU), пять обязательных случаев + два
дополнительных для покрытия ветки `char_at` (\u-escape и backslash-на-конце):

```
before-fix (fixed json.nv в дереве):
"{ broken"              -> Err as expected: UnexpectedChar
""                      -> Err as expected: UnexpectedEof
"\"unterminated string" -> Err as expected: UnexpectedEof
"12."                   -> Err as expected: InvalidNumber
"\"bad escape \\"       -> Err as expected: UnexpectedEof
ALL FIVE CASES SURVIVED — NO CRASH
EXIT CODE: 0
```

Дополнительно (отдельный standalone-пробник, обрыв внутри `\u`-escape):
```
"\"\\u12" -> Err as expected: UnexpectedEof
SURVIVED — NO CRASH
EXIT CODE: 0
```

Все пять требуемых случаев + оба дополнительных: **Err, без падения, exit 0.**

### 3. Проба «подсунь заведомо негодное»

`char_at` временно возвращён к `char`+`!!` (три call site — тоже к оригиналу):

- (а) диагностика вернулась: `nova check std/src` → `PASS: 146  FAIL: 31  WARN: 54`,
  114 строк `E_BANG_REQUIRES_FAIL` (совпадает с исходным до-фикс состоянием байт-в-байт по
  количеству уникальных локаций — 9).
- (б) крах воспроизведён: т.к. обычный разбор не достигает `char_at`'s `None`-путь
  напрямую (все три call site защищены предшествующим `@peek()`/`@advance()` — см.
  «Уточнение механизма» выше), для прямой пробы добавлена временная exported-обёртка
  `json_probe433_eof_char_at(s str) Fail[()] -> char`, вызывающая
  `Lexer.new(s).char_at(s.bytes().len())` — ровно "позиция за концом ввода" из текста
  дефекта. Прогон:
  ```
  before call
  nova: unhandled Fail: RuntimeNoneError
  EXIT CODE: 127
  ```
  Программа не дошла до `"after call"` — **необработанный отказ воспроизведён**.

  После пробы `char_at`/call sites/сигнатуры `Json.parse`/`try_from` восстановлены в
  фиксированное состояние (сверено `diff` с бэкапом — идентично), временная
  `json_probe433_eof_char_at` удалена вместе с восстановлением файла из бэкапа.

### 4. Регресс

- `nova test std/src/encoding`: `PASS: 7  FAIL: 1  SKIP: 23`. `json_test` — **PASS**.
  Один `CC-FAIL`: `std/src/encoding/serde/decode_errors_test` — C-codegen ошибка типа
  (`assigning to 'NovaOpt_Nova_Vec____nova_str_p' from incompatible type
  'NovaOpt_Nova_Vec____nova_int_p'`, `decode_errors_test.c:18605:21`). **НЕ связан с
  №433** — тест не ссылается на `char_at`/`ParseJsonError`, ошибка это generic-mono
  C-type clash в `serde`'s `json_decode[T]` для record с полями `int`+`str`. Ранее была
  НЕВИДИМА, т.к. вся цепочка не проходила тайпчек до codegen (блокировалась на
  `E_BANG_REQUIRES_FAIL` раньше, чем C-codegen успевал её поймать) — подтверждено:
  прогон того же теста на ИСХОДНОМ (до-фикс) `json.nv` даёт `CODEGEN-FAIL` на
  `E_BANG_REQUIRES_FAIL`, т.е. раньше падал ЕЩЁ РАНЬШЕ, эта C-ошибка была замаскирована.
  **Новый носитель для отдельного номера — владелец, доложено, не фикшу в этом окне**
  (вне класса Result+`!!`, не блокирует json/jwt — вне срочного объёма).
- `nova test std/src/crypto`: `PASS: 5  FAIL: 0  SKIP: 5`. `jwt_test` — **PASS**.

### 5. Фикстура

`spec_tests/conformance/json_parse_broken_input_returns_err.nv` — 6 `test`-блоков (правило
1, позитив, плоский корень, `module spec_tests.conformance`, без `fn main`): `"{ broken"`,
пустая строка, обрыв внутри строки, обрыв внутри числа, обрыв внутри `\u`-escape, обрыв
сразу после `\` на конце ввода. `nova check` на файл — `PASS: 1` (пиры того же модуля
подтягиваются автоматически, типизация всей плоской папки чиста). Мега-CU `nova test` на
этот путь НЕ гонялся (тянет весь плоский пакет = мега-CU — CPU-дисциплина, авторитетный
гейт — у владельца); каждое из 6 утверждений фикстуры независимо перепроверено
standalone-сборкой (см. §2) — совпадают дословно.

## Коммиты (в worktree `nova-p433`)

1. `fix(221.1, №433): Lexer.char_at возвращает Option, не бросает !! на конце ввода` —
   `std/src/encoding/json.nv`.
2. `test(221.1, №433): поведенческая фикстура — битый JSON даёт Err, не крах` —
   `spec_tests/conformance/json_parse_broken_input_returns_err.nv`.

## Не в объёме этого окна (номер — владелец)

- `std/src/encoding/serde/decode_errors_test.nv` — CC-FAIL, generic-mono C-type clash в
  `json_decode[T]`, новооткрывшийся (был замаскирован №433), не относится к классу
  Result+`!!`. Требует отдельного расследования generic-codegen для record с
  разнотипными полями (`int`/`str`) через сериализатор.
