# 137: сравнение трёх альтернативных runnable-входов

По переданному интегратором решению владельца typed Fail в 137 не расширяется. Проверены **два** существующих автономных entry из parser-fallback-корзины 135, без изменения их исходников: `examples/basics/string_literals.nv` (L048) и `examples/tour/collections_tour.nv` (L103). Полный корпус не повторялся.

## Замер

Сохранённый локальный combined seed 135, copied install и patched-scratch-driver-v2 — те же, что в основном отчёте. Исполнитель: `compare-alternatives.py`, аргумент — сессионный scratch из report.md. Machine watch `1793544913662-cg7k91`, 48 секунд, rc=0 скрипта означает завершение обследования, **не успех программ**.

| Вход/проба | Стадия | rc | Диагностик |
|---|---|---:|---:|
| Исходный string_literals.nv | build | 1 | 19 |
| Исходный collections_tour.nv | build | 1 | 8 |
| quoted | emit | 0 | 0 |
| continuation | emit | 0 | 0 |
| backtick | emit | 1 | 2 |
| tagged | emit | 1 | 2 |
| tag-direct | emit | 0 | 0 |
| lambda | emit | 1 | 1 |
| map | emit | 1 | 1 |
| builder | emit | 0 | 0 |

В отрицательных минимизациях: `E_NOVAC_SUBSET`, дословно `novac did not parse this -- and it cannot tell whether the form is one it does not read yet or the text is malformed; this is the parser's fallback, narrowed form by form`. Позиции: backtick `20..21`, `26..27`; tagged `76..77`, `82..83`; lambda `19..20`; map `84..85`. Исходные build отказывают ещё на emit; exe не созданы и не запускались. **Новых build+run PASS — 0.**

Сырые выводы и источники проб: `<scratch>/alternatives/{name}.stdout`, `.stderr`, `.nv`, сводка `results.json`. Источник воспроизведения всех проб сохранён в `compare-alternatives.py`; исходные примеры читаются непосредственно из checkout и не переписываются.

## Строковый пример — кратчайший из двух по доказанной цепочке

Минимизация backtick: `fn main() { println(\`hello\`) }` отказывает, обычные кавычки проходят emit. Тегированная форма `tag\`hello\`` отказывает даже при простой функции `tag(parts []str, args []str) -> str { parts[0] }`, а её прямой вызов `tag([]str.of("hello"), []str.new())` проходит emit. Отдельно проходит emit тело настоящего tag с `consume StringBuilder`, циклом, append и возвратом builder как str. Это снимает ранние подозрения на continuation и builder, но **не доказывает их runtime-поведение**.

Нормы проверены через read-spec: D467 (continuation, backtick, три escape, интерполяция, нормализация CRLF/отступов) и D48 (обычный вызов тега с parts/args). В Carina отсутствует backtick-дверь лексера и template-форма дерева/парсера. Полный ремонт включает:

1. Lossless backtick-лексинг с разделением текста и интерполяций; не подмена исходного текста кавычками.
2. Проверку/нормализацию содержимого по D467 с отдельным набором escape; голая форма собирается через Display.
3. Tagged-вызов с типизацией parts/args и контекстом пустого массива; сохранить исходную геометрию дерева и однократность вычислений.
4. Точный stdout исходного примера, LF/CRLF, escape/interpolation/tag и саботаж; source→C→exe локального сида.

Это существенно уже typed Fail: новые runtime-эффекты/замыкания не требуются. Однако готовой template-двери, куда достаточно добавить условие, нет. Сквозной объём lex/parse/check/lower/emit передан интегратору до расширения. Не утверждается, что на другом, не проверенном входе не существует более короткого класса.

## Collections — минимум два независимых отсутствующих механизма

Лямбда `ro f = |x| x + 1` отказывает отдельно от map; map `["a": 1]` отказывает отдельно от лямбды. D22 и D108 — уже принятые формы.

Самостоятельно сверенные двери: `parse/expr.nv:731–748` всегда собирает `ArrayLit`, не пары key/value; `check/params.nv:75–85` читает callback-тип для handed-сигнатур, но compiled function-typed параметр сознательно отвергает. Отсутствуют lambda-узел/разбор и сквозное представление/вызов callback. Помимо этого полный пример требует iterator/filter/count, flat_map и Display контейнеров. Один parser-фикс не даёт обоснованного пути к runnable PASS. Этот носитель длиннее строкового и не выбран для реализации.

## Граница результата

Исправлений языка/компилятора нет; положительные минимизации — только emit-контроли. RED/GREEN исправления, реальный run, прирост и сохранность прежних PASS ещё не доказаны. Typed Fail №1948 остаётся открытым. Продолжать перебор других входов или реализовывать оба механизма collections без решения об объёме не следует.

## Третий вход — tail-without-value, PostgreSQL smoke

По следующему поручению интегратора проверен **ровно один** вход другой корзины: `nova-postgres/examples/smoke/src/main.nv`. HEAD пакета `8a46520269cf430e3f9d7a78ad9ae729ed23af8a` совпадает с 135. В entry нет `!!`, lambda или template (backtick встречается только в комментариях). Источник скопирован вместе с src и манифестами в мой scratch; исходный checkout и его lock не менялись. Скрипт `compare-tail.py` сохраняет исходные байты, сборка использует ту же path-зависимость на родительский пакет.

Первый запуск: build rc=1 **до компилятора**, `E_DEP_PATH_OUTSIDE_REPO`, поскольку у копии не было git-границы для `path = "../.."`. Это ошибка подготовки контура, не результат на языке. В Result[int,str]-минимизациях контрольные while/direct дополнительно упёрлись в отсутствующий shell-instance `NovaRes_nova_int_nova_str`. Поэтому второй адресный запуск исправил только контур (изолированный `git init` без remote/коммитов) и убрал контейнер из минимизаций. Исходный реальный entry не менялся.

Финальный watch `1793546554638-w9ihih`, 14 секунд; исходный watch `1793546394886-5l5xnj`, 19 секунд. Полные выводы: scratch `tail-alternative/` и `tail-alternative-v2/`; staged-дерево сохранено флагом `--keep-artifacts` в `install135/target/carina-build/main.45640.src/`.

```text
smoke-build rc=1 diagnostics=63
tail-loop-emit rc=1 diagnostics=1
tail-while-emit rc=0 diagnostics=0
tail-direct-emit rc=0 diagnostics=0
```

Первая диагностика реального entry снова точно совпала с 135: `E_NOVAC_SUBSET`, span `2635..2636`, `fn declares a return type but its body ends without a value`. В сохранённом staged entry проверены строки 62–85: функция read_one заканчивается `loop`, выходы — `return`.

Минимальный корень:

```nova
fn read_one() -> int { loop { return 42 } }
fn main() { println(read_one()) }
```

Получает то же сообщение, span `28..29`. Контроли — тот же текст с `while true` вместо `loop` и с прямым `return 42` без цикла; оба дают emit rc=0, но это **не исправление исходного smoke** и не run-PASS.

D485 §3 (`spec/decisions/03-syntax.md:13957–13963`) требует never для loop без своего break; цитата сверена самостоятельно после read-spec. Корень уже зарегистрирован как **№1089**: loop пока лексится именем и не становится узлом цикла. `tail_rules.nv:42–66` умеет `while true` без break, IR уже имеет LoopHead.Forever; историческое утверждение №1089 об отсутствии Forever устарело. Минимальный ремонт этого корня — source-language loop через parse/tree/check/lower с контролем break/never, а не разрешить любой хвост без значения.

### Почему этот локальный корень ещё не даёт полного smoke PASS

В том же свежем build остаются 62 другие диагностики. Их число **не означает 62 независимых дефекта**, но все считать каскадом loop нельзя: отказы есть и в отдельных исходниках без loop. Например, `src/md5.nv:19` имеет параметр `salt [4]u8`; файл вообще не содержит цикла, а сборка получает в нём ошибки чтения сигнатуры/параметра. В `decode.nv` отказывают размещения record-форм вариантов, в `encode.nv` — объявление namespace-типа/типовая форма. Их точные корни здесь не минимизированы и не заявлены исправимыми одним loop-фиксом. Полный run этого реального примера также требует PostgreSQL с известной конфигурацией, проверка поведения — `SMOKE OK` после startup/auth/SELECT 1/ReadyForQuery; простое завершение с `SMOKE FAIL` не приёмка.

## Итоговое сравнение трёх

| Реальный вход | Свежий baseline | Доказанный первый корень | Объём до обещания полного PASS |
|---|---|---|---|
| string_literals | build rc=1, 19 диагностик | backtick/tagged; обычный tag и builder дают emit rc=0 | D467/D48 lex/parse/типизация и сборка parts/args; строковый объём интегратором запрещён к расширению |
| collections_tour | build rc=1, 8 диагностик | отдельно lambda/callback и map-literal | как минимум две разные сквозные формы; callback ABI и контейнерные вызовы ещё не проверены |
| postgres smoke | build rc=1, 63 диагностики | source-language loop, №1089; while/direct контроли emit rc=0 | loop плюс ещё не минимизированные отказы зависимостей вне циклов, затем настоящий PostgreSQL run |

Итог не распространяется на все 16 tail-носителей: проверен один выбранный entry, а не вся корзина. Все три baseline красные, нового build+run PASS **0**. Короткой доказанной цепочки до полного PASS ни для одного не получено. Расширение остановлено на этой сравнительной границе, без правок исходных примеров/компилятора и без подмены результата другой диагностикой.
