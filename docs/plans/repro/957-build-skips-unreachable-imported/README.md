# Проба: `nova build` не судит недостижимую функцию ИМПОРТИРОВАННОГО модуля

Реестр 221.1, строка №957 — заводит интегратор (окно nova-f6, его канал; решение
2026-09-05: «строку напишу после вашего пуша, в неё войдёт таблица и обе двери»);
проба — окна 274. Его же перемер уточнил границу: не «`build` против `check`», а
«команды ОТ ENTRY (`build`, `check <файл>`) судят только достижимое; каталожные
(`check <каталог>`, `test <каталог>`) — всё», и позже — что недостижимая функция не
судится и в самом файле входа. Семья №669, рядом с №760 (форма `return t` у
`ro`-связанного newtype — сама диагностика законна, здесь вопрос не о ней, а о том,
КТО и ГДЕ её ставит).

## Что измерено (2026-09-05, `nova-cli/target/release/nova.exe` вершины `d380a8dcf`)

Каталог рядом: `main.nv` импортирует `./helper.{used}`; `helper/helper.nv` объявляет
`export fn bad(t TyId) -> TyId => t` при `export type TyId int` — форму №760, которую
`nova check` отвергает `E_READONLY_COERCE`. `bad` из `main` НЕ вызывается.

| команда | результат |
|---|---|
| `nova check helper/helper.nv` | `E_READONLY_COERCE` ×1 |
| `nova check main.nv` | 0 диагностик |
| `nova build main.nv -o probe.exe` | **`built`, rc=0, exe создан** |
| `nova build single.nv` (та же форма в ОДНОМ файле, `bad` не вызывается) | `E_READONLY_COERCE` — сборка отказана |

Значит: недостижимая функция в самом файле входа судится `build`'ом (замер интегратора
2026-09-05 на однофайловой пробе — совпадает со строкой `single.nv`), а недостижимая
функция в ИМПОРТИРОВАННОМ модуле — нет. `nova check` того же файла и сборка test-CU
(`nova test <файл>_test.nv`, где файл входит в CU теста) её судят. Первое наблюдение
было на дереве novac: `nova build novac/src/main.nv` собрал `sem/mangle.nv` с голым
`return t` в `impl_of_proto` (rc=0), а `nova check novac/src/sem/mangle.nv` и
`nova test novac/src/sem/mangle_test.nv` покраснели на той же строке; перемер на вершине
с временно дописанной негодной функцией дал то же самое (check ×1, build rc=0, test FAIL).

## Почему это тяжелее «трёх драйверов»

Программа, которую `check` отвергает, СОБИРАЕТСЯ — и не потому, что драйвер иначе
понимает правило, а потому, что часть кода не судится вовсе: экспортированная функция
библиотечного модуля, у которой пока нет вызывающего, компилируется без проверки тел.
Для библиотеки (std, пакеты) это норма жизни: половина её функций недостижима из любого
одного `main`.

## Воспроизвести

Файлы лежат как `.nv.txt` (как в соседних каталогах repro — чтобы их не подхватывали
корпусные прогоны и стражи); перед запуском скопировать в пустой каталог, сняв `.txt`:

```sh
P=target/probe760; mkdir -p $P/helper
cp docs/plans/repro/957-build-skips-unreachable-imported/main.nv.txt $P/main.nv
cp docs/plans/repro/957-build-skips-unreachable-imported/helper/helper.nv.txt $P/helper/helper.nv
cp docs/plans/repro/957-build-skips-unreachable-imported/single.nv.txt $P/single.nv
nova check $P/helper/helper.nv        # E_READONLY_COERCE
nova build $P/main.nv -o $P/probe.exe # built -- ожидалось: тот же отказ
nova build $P/single.nv -o $P/s.exe   # отказ (контроль: однофайловый случай судится)
```

Первая проба с модулем по имени `m` (`import ./m.{used}`) упёрлась в ДРУГОЙ отказ
оракула — `std/src/collections/hash_map/core.nv:146: [E7401] no function insert in
module m` — имя пользовательского модуля `m` столкнулось с внутренним алиасом std;
отдельная находка, здесь не разбирается (каталог `helper` её обходит).
