# 137 — Получить новый build+run PASS на реальном примере Carina 0.3

## Результат: НЕ ЗАВЕРШЕНО

Новый build+run PASS **не получен**, прирост доказанных PASS — **0**. Исправления компилятора нет. Выделен барьер существующего `examples/effects/serde_fs_build.nv`: постфиксный `!!` не разбирается; его нормативное раскрытие требует типизированного `throw`, который этот сид также не разбирает. Продолжение потребует реализации семантики распространения ошибки, а не локального исправления parser fallback. Подменять `throw` строкой, panic или игнорированием ошибки нельзя.

## РЕПРО

Выбор сделан по `135.../evidence/next-walls.md`, `table-118.tsv`, полным логам L048, L064, L074, L093, L104 и исходникам. L064 — самостоятельный BUILD-сентинел serde+fs с одной диагностикой; L048 содержит несколько строковых форм, L074 не имеет `main`, сетевые entry требуют внешнего сценария, прочие просмотренные кандидаты имеют несколько препятствий. Это основание выбора, не доказательство глобального минимума по всем 34 входам.

Сессионный scratch: `%LOCALAPPDATA%/Temp/opencode/ses_mvxsi0zpb6qi6xl0ef/`.
Материалы 135 получены у интегратора из соседнего `ses_efdd2dc2effePXj0L8WPrxkIJN/task135-preserved`. В мой scratch скопированы install135 без target, драйвер v2 и локальный ZIP. Сохранённый контур интегратора не менялся. Для кэша использован отдельный LOCALAPPDATA внутри моего scratch.

Команда baseline, с путями относительно scratch:

```text
NOVA_RT_DIR=<scratch>/install135/nova_rt
NOVA_CG_INCLUDE=<scratch>/install135
NOVA_CARINA_SEED_ZIP=<scratch>/nova-carina-0.2.2-135combined-windows-x86_64.zip
LOCALAPPDATA=<scratch>/localappdata
<scratch>/nova135.exe build <checkout>/examples/effects/serde_fs_build.nv -o <scratch>/serde.exe --toolchain clang --timeout 300
```

ZIP SHA256: `c727288f301b4b8849bffe4d6e75b7bc9f55589472cfec97029d70521d16fb69`. `verify-seed PASS`, source `3729abcce42ec2d94efc898ec6eafe91ff6210d5+task135-combined-130-131-134`. Это **patched-scratch-driver-v2**, не поставляемый драйвер и не опубликованный сид. Provenance существующего сида — MANIFEST/отчёт 135; нового бинаря из исправленного кода нет.

Результат исходного реального входа: `BUILD rc=1`, emit exit 1, `E_NOVAC_SUBSET`, span `1384..1384`, дословное сообщение:

```text
novac did not parse this -- and it cannot tell whether the form is one it does not read yet or the text is malformed; this is the parser's fallback, narrowed form by form
```

Сырые выводы: scratch `baseline.txt`, `min-results.txt`, `barrier-results.txt`; для каждой минимизации `<name>.nv`, `<name>.stdout`, `<name>.stderr`. Постановки очереди: `1793536885162-1qggbl` (32 с), `1793539395434-7f5w5x` (21 с), `1793539524826-ws2m4u` (17 с). Все тяжёлые команды шли через machine queue.

## ТОЧКА И НОРМА

Норма прочитана через spec-reader, цитата проверена непосредственно: D85, `spec/decisions/04-effects.md:4812–4818`, `expr!! ≡ match expr { Ok(v) => v; Err(e) => throw e }`. Для Option — Some/None с RuntimeNoneError. `with Fs` не перехватывает Fail.

`novac/src/parse/expr.nv:274–350`: postfix-цепочка обрабатывает доступы/индексы/`?`/`??`, но не `!!`. Поиск `throw` по `novac/src` не обнаружил узла/типизации/эмиссии throw; `lex/lex.nv:379` явно называет его долгом. Комментарий сам по себе не служит доказательством: ниже приведена отдельная проба.

## ФИКСТУРЫ / МИНИМИЗАЦИЯ

Дословные строки результатов:

```text
plain EMIT rc=0
bang EMIT rc=1
option EMIT rc=0
with EMIT rc=0
serde_without_bang EMIT rc=1
serde_throw EMIT rc=1
serde_success_control EMIT rc=0
```

- plain: `fn main() { println(42) }`.
- bang: `fn main() { println(Some(42)!!) }`; fallback, span `30..30`.
- option: `fn main() { println(Some(42) ?? 0) }`.
- with: импорт `std.fs.{real_fs}`, `fn main() Fs Os -> () { with Fs = real_fs() { println(42) } }`.
- serde_without_bang: копия реального входа с удалением только `!!`; следующий отказ `outside the subset: println covers int, str, bool and f64 today` — печатался Result, это не исправление.
- serde_throw: в копии реального входа вызов на строке 29 раскрыт в `ro encoded = json_encode(Probe { a: n, name: "serde+fs" })`, затем `ro text = match encoded { Ok(v) => v Err(e) => throw e }`, затем `println(text)`. Отказ `syntax error: expected \`=>\`` (span `1452..1452`).
- serde_success_control: то же раскрытие, но Err-плечо — строка `"error"`; emit rc=0. **Этот контроль меняет поведение и не принимается как решение или новый PASS.**

Все положительные строки здесь — только **emit**, C не компилировался и exe не запускался. Минимизации не являются регрессионными тестами исправления. Результат сериализации/обработки ошибки запуском не подтверждён.

## ФИКС / САБОТАЖ / КЛАСС

Фикса нет; саботаж RED/GREEN исправления не выполнен. Диагностические контроли показывают границу отказа, но не заменяют требуемый саботаж.

Поиск `!!` в examples обнаружил также `flagship/aggregator/src/main.nv`, `flagship/http_proxy_chain/src/main.nv`, `real_world/orm_demo.nv`, `flagship/aggregator/regressions/serde_encode_pointer_op/serde_encode_pointer_op.nv`. Это кандидаты класса, не повторные доказательства одного корня: полные цепочки этих входов не прогонялись. Корзины 18/16 не объявляются единым классом. Прежние PASS повторно не проверены, поскольку исправления и первого нового PASS нет.

## std/src / НОРМА / ИНВАРИАНТ / CI

- std-check: не применимо к внесённым изменениям — компилятор и std не менялись; `nova check std/src` до/после не запускался. Это не утверждение PASS.
- spec: язык не меняется, новый D-блок не применим.
- invariant: новых инвариантов и стражей нет.
- registry: открытый дефект №1948; номер назначен интегратором письмом 2026-11-01. Релизный вердикт — интегратору.
- gate/ci: локальные полные гейты не запускались. Кандидат integrate/t137 не создан; CI и LANDED отсутствуют. Приёмка исправления невозможна.
- Проверки отчётной правки: `git diff --check` — без замечаний; `reproduce.py syntax PASS`; `check-registry-single-verdict` и `check-registry-routes` — ok. `check-registry-entry-shape: FAIL`: одна запись TBD, номер запрошен у интегратора. Это открытая проверка, не зелёный гейт.

## ВЕТКА/КОММИТ / ЧТО НЕ СДЕЛАНО

Ветка `task/137-poluchit-novyy-build-run-pass-na-realnom`. Исходная вершина `be6281534ab098a8ffa2219f9f9ccff20c30537e`; после `git fetch origin main` целевой `origin/main=fb97763831319fe1f0eafb63d805d8360a3e801c` уже является предком (`merge --ff-only`: `Already up to date`). Различий HEAD..origin/main по novac/src и реальному носителю нет.

Критерии дословно: «Существующий вход с baseline FAIL становится build+run PASS с ожидаемым поведением; минимальный регрессионный тест и саботаж RED/GREEN; provenance бинаря из окончательного кода; результаты других носителей и сохранность прежних PASS; если не достигнуто — честная незавершённость, смена текста диагностики не приёмка».

Первый и последующие положительные критерии **не выполнены**. Сделаны только baseline, минимизация, сверка нормы и фиксация конкретного следующего барьера. Приёмка глазами, потому что выбор носителя и граница расширения задачи требуют оценки отчёта; она не может объявить отсутствующий build+run PASS полученным.

## Передача после перезапусков

Основной отчёт сдан штатным `crew_task submit`, коммит `6a88040aa`. Интегратор принял его как незавершённый результат, не как приёмку цели; назначил №1948. Все три запущенных watch завершены, активных команд этой задачи не осталось. Следующий шаг — согласовать объём ремонта типизированного распространения ошибки. По письму интегратора новые тяжёлые прогоны, main и CI приостановлены; сохранено только оформление номера. Исторический FAIL проверки TBD выше относится к состоянию до назначения номера. После назначения номера страж выявил английский заголовок CLASS вместо требуемого КЛАСС; оформление поправлено, `check-registry-entry-shape ok` (343 <= 343).
