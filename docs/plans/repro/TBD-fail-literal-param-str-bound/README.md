# Repro №TBD — параметр `fail(e)` handler-ЛИТЕРАЛа связан в кодогене как `nova_str` при любом `E`

Найдено 2026-11-07 окном opencode при сборке проверяемых примеров к ответу
в nv-lang/nova#6. Замерено на nova-cli release HEAD, Windows, clang.

## Клетки

| Файл | Форма handler'а | Результат |
|---|---|---|
| `pos_lambda_shorthand.nv` | `with Fail[DbErr] = \|e\| interrupt Err(e)` | **PASS**, stdout `same account caught` — контроль, форма рабочая |
| `cc_fail_typed_literal.nv` | `effect Fail[DbErr] { fail(e) -> never { interrupt e is SameAccount } }` | **CC-FAIL**: `member reference type 'nova_str' is not a pointer`, `no member named 'tag' in 'nova_str'` — `e` эмитится как `nova_str` |
| `runfail_literal_wrap_err.nv` | `effect Fail { fail(e) -> never { interrupt Err(e) } }`, далее `e is SameAccount` | **RUN-FAIL (тихо)**: `e is SameAccount` ложно, `e is Other` тоже ложно — значение скорrupted; `check` чист |
| `crash_literal_record_param.nv` | то же, но `DbErr` — запись `{ msg str }` | **КРАШ**: `nova: out of memory`, exit 0xC0000409 |

Проверено также: параметр связан как `nova_str` даже при enum из одних
unit-вариантов (`probe6k` в сессии: `passing 'nova_str' to parameter of
incompatible type 'NovaValue_DbErr'`).

## Прочитка

Чекер типизирует `e` правильно (`e is SameAccount` проходит check), кодоген
подставляет `nova_str`. Похоже на хардкод «ошибка = строка» на пути
handler-литерала `effect Fail { fail(e) ... }` (bare `throw "msg"` даёт
`str`, D-норма); lambda-шортхенд `|e| ...` идёт другим путём и честен.
Соседство: №680 («съедаемый бросок из руки обработчика»), #1649 (отказ
match-руки, на который ссылается `examples/real_world/orm_demo.nv:537`).

Конформенс-фикстура `spec_tests/conformance/tx_rollback.nv` класс не ловит:
её handler не использует параметр (`fail(_e)`), обход тривиален.
