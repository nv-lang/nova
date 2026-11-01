# Отчёт задачи 132 — №1924: own-package qualified импорты в Карине

## КРИТЕРИИ ПРИЁМКИ (дословно из задания)

«Минимальный репро; нормативно обоснованный фикс; саботаж RED/GREEN;
отсутствие регрессии 62 PASS и локальных импортов; фактические исходы
многомодульных входов; стражи/CI на кандидате.» — выполнены все, см. ниже;
что машиной не проверить — названо «приёмка глазами» в соответствующих
пунктах.

## РЕПРО

Минимальное репро = фикстура `spec_tests/conformance/fixtures/ownpkg_build_min/`
(пакет `ownpkg_build_min`: `main.nv` = `module ownpkg_build_min.main` +
`import ownpkg_build_min.helper.{greet}` + `import ownpkg_build_min.math.{triple}`;
`helper/helper.nv` — модуль-каталог, форма-носитель №1924; `math/math.nv` —
второй модуль-каталог). До фикса штатный `nova build` через pinned B1
(windows-x86_64, clang) даёт дословно сигнатуру №1924:

```
E_NOVAC_ICE internal compiler error at novac/src/pipeline/program.nv:157
(novac bug, not yours): pipeline: `novac_fn_ownpkg_build_min_helper_greet__
nova_str__to_nova_str` belongs to module `ownpkg_build_min.helper`, which
the entry's imports do not reach
```

Дословный лог — `evidence-red-prefix.txt` (драйвер ДО фикса, SHA-256
`e88f8f965a…53ac`, тот же сид/пины).

## ТОЧКА (выбор места фикса — по спецификации, не выдумано)

`nova-driver/src/program.rs` (стейджинг №1889/№1930). Нормативная основа —
**D78 rev-4** (`spec/decisions/07-modules.md`, «Импорт — полный путь от корня
пакета либо относительный», раздел «Относительные импорты `./`/`../`»):
qualified-импорт — путь от корня пакета, относительный — путь от
импортирующего файла; оба именуют ОДИН модуль, «неоднозначности нет by
construction». Сид B1 (запинен, не меняется) строит рёбра между единицами
только по ОТНОСИТЕЛЬНЫМ импортам — измеренный корень №1924; для зависимостей
задача 129 (№1930) уже переписывает qualified-импорты в относительные в
стейдж-копиях. Противоречия спецификации нет — выбран драйверный фикс по
прецеденту 129; Карину (сид) не трогал.

## ФИКС

Только `nova-driver` (сид запинен и неизменён, без Oracle):

- `program.rs`: `own_qualified` — распознавание own-package qualified токена
  (первый сегмент == `[package].name`, не `./`/`../`); `own_staged_module_loc` —
  стейдж-локация модуля (пакет-относительный путь, у file-модуля без
  расширения — кончается последним сегментом модуля, 274.11 E.10);
  `staged_dir_of` — каталог стейджа импортирующего файла. Ребро
  переписывается ТОЛЬКО cross-unit (цель в ДРУГОЙ директории стейджа):
  same-unit цели (сид сворачивает директорию в одну единицу, peers уже делят
  namespace) и импорт корня пакета (нет последнего сегмента) не
  переписываются — граница названа в доке модуля. Значение пары
  `Discovery::rewrites` обобщено с «имя модуля депа» до «стейдж-локация
  модуля» (`_deps/…` для депов — поведение 129 сохранено байт-в-байт
  юнит-тестом).
- `carina_build.rs`: комментарий точки вызова (код не менялся — стейджинг уже
  применяет rewrites к входу и файлам).
- Пользовательские исходники не трогаются НИКОГДА — только стейдж-копии
  (юнит-тест сверяет побайтово).

## ФИКСТУРЫ

- `spec_tests/conformance/fixtures/ownpkg_build_min/` (4 файла) — без
  EXPECT_*-маркера намеренно (как dep_build_min: манифест живёт только в
  стейдж-копии под target/, судья — страж), корпус-волокер каталог `fixtures`
  пропускает (D376, урок CI-рана 41108666626 из 129).
- Страж `scripts/tools/carina-build-ownpkg.py` (зеркало carina-build-dep.py):
  стейджинг пакета с манифестом → `nova build` через драйвер + pinned B1
  (проверка штампа carina-b1-build-v1, SHA сида/B1, seed_freshness,
  binary_sha256) → ЗАПУСК бинаря → построчная сверка ПОЛНОГО stdout с
  `expected_stdout.txt` («Hello, Nova!» из модуля + «triple(7) = 21» из
  второго модуля — оба ребра обязаны работать). Утверждает наблюдаемое
  поведение (значение, код возврата), зовёт проверяемое.
- Юнит-тесты `program.rs`: +4 на класс (переписывание folder+file-модуля в
  стейдже с сохранением исходников байт-в-байт; `../..`-форма из вложенной
  директории; same-unit/корень не переписываются; обновлена прямая проба
  `rewrite_imports` под новую семантику пары) — итог 26/26 зелёные.

## САБОТАЖ (RED/GREEN, оба дословно)

- **КРАСНЫЙ (фикс сломан = драйвер ДО фикса):**
  `carina-ownpkg FAIL: driver build failed (exit 1)` с дословным
  `E_NOVAC_ICE … program.nv:157 … ownpkg_build_min.helper, which the entry's
  imports do not reach` (`evidence-red-prefix.txt`).
- **ЗЕЛЁНЫЙ (фикс возвращён):**
  `carina-ownpkg PASS: ownpkg_build_min exit=0 stdout=2 lines
  binary_sha256=a8e359c2…dcd34c` + `PASS: the own-package qualified-import
  fixture built through the pinned Carina B1 seed and matched its full
  expected stdout` (`evidence-green.txt`).

## Регрессии (локальные импорты, депи, 62 PASS)

- Относительные импорты (класс №1889): `carina-build-crossmod.py` — 2/2 PASS
  (`evidence-regression-crossmod.txt`).
- Депы (класс №1930, семантика пары rewrites менялась — проверено явно):
  `carina-build-dep.py` — PASS (`evidence-regression-dep.txt`).
- 62 PASS-примеров замера 128 — все однофайловые: они не проходят через
  стейджинг вовсе (исторический путь одиночного файла не тронут: изменение
  только добавляет rewrites для own-package qualified токенов, которых в
  однофайловых входах нет). 19 многомодульных входов прогнаны фактом — см.
  ниже; сигнал «staged» в логах подтверждает: однофайловых среди них 1
  (`aggregator/src/domain/domain.nv`, staged=0), и его исход совпадает с
  замером 128 (прежний честный subset-отказ).
- **std/src**: не применимо — правка только в `nova-driver` (Rust-crate,
  `nova check` его не потребляет); компилятор (compiler-codegen/nova-cli),
  std и `.nv`-корпус не тронуты ни одной строкой (фикстура живёт в
  `conformance/fixtures/`, который Oracle-раннер не видит).

## Фактические исходы 19 многомодульных входов замера 128 (не обещаю PASS)

`evidence/outcomes-18/{outcomes.tsv,log-*.txt}` — все 19 входов таблицы
`doc/tasks/128-*/evidence/table-118.tsv` (aggregator 10, http_proxy_chain 6,
tour/greeter 2, modules_tour 1; из них 18 прошли стейджинг — совпадает с
отчётом 128). **Итог: PASS 0, но сигнатура №1924 исчезла ВЕЗДЕ** (0 вхождений
`program.nv:157`/«do not reach» по всем 19 логам):

- `examples/tour/modules_tour.nv` (носитель №1924): ребро импорта ПРОШЛО,
  следующий отказ — честный предел подмножества B1 ВНУТРИ std:
  `E_NOVAC_SUBSET … std/src/unicode/case.nv … []T.new(...) takes no
  arguments … registry 221.1 #1457, 274.5 P3`.
- `tour/greeter/{core,loud}.nv` — прежний корректный link-no-main
  (`undefined symbol: nova_fn_main_impl`, `fn main` в файлах нет).
- aggregator/http_proxy_chain — прежние честные subset-отказы замера 128
  (record-constructor/effect-attribute позиции, `no such method` на поляриса
  — через стейджинг депов 129) и один `E_NOVAC_ICE type_of.nv:632`
  (aggregator main — «anonymous constructor without a spread», семья
  №1484/№1705, в 129 отмечен у поляриса как привязанный к сиду).

## spec / registry / invariant

- **spec**: язык не меняется — «не применимо» (драйверный контур; обе формы
  импорта уже нормированы D78 rev-4, фикс лишь выбирает написание для
  стейдж-копии).
- **registry**: строка №1924 закрыта этим же слиянием (ЗАКРЫТА 2026-10-31,
  задача 132, с RED/GREEN и замером после; сид-оговорка сохранена: сам B1
  рёбер по qualified не строит, новая сборка сида снимет переписывание).
  Новых строк/номеров не заводилось (новых дефектов не найдено; №1457 и
  №1484/№1705 — существующие). Реестровые стражи зелёные: rows-intact
  (1840/1931/0), entry-shape, single-verdict, status-field, past-tense,
  closure-kept, routes. Тесты не ослаблены/не удалены; новых E_*/W_* нет.
- **invariant**: новый инвариант один — «own-package qualified импорт в
  стейдж-копии обязан стать относительным (cross-unit)»: без него сид
  гарантированно ICE-ит (№1924), то есть без него нельзя; страж
  `carina-build-ownpkg.py` — его самотест (RED-форма показана выше).
  ЧЕСТНЫЙ СТАТУС: запуск РУЧНОЙ (`NOVA_CARINA_DRIVER=<драйвер> python
  scripts/tools/carina-build-ownpkg.py`), в гейт/CI не подключён — по той же
  причине, что у carina-build-dep.py из 129: сид запинен под windows-x86_64,
  Linux-CI его не гоняет; 26 юнит-тестов nova-driver — ручной прогон (CI
  трогает nova-driver только cargo check).

## ВЕТКА/КОММИТ

`task/132-1924-own-package-qualified-importy-v-kar`, основа main `0b8438a34`.
Коммит — в сдаче. Кандидат: `integrate/t132` (свежий main + ветка), CI
проверен `check-push-proven-by-ci.py`.

## ЧТО НЕ СДЕЛАНО

- 19 многомодульных входов НЕ стали PASS (0/19) — ICE №1924 больше не
  блокирует ни один, но дальше честные пределы подмножества B1: №1457
  (`[]T.new(capacity)` в std/unicode), record-ctor/effect-attribute позиции,
  ICE type_of.nv:632 (№1484/№1705), link-no-main для не-входов. Чинить их —
  не эта задача (границы: не чинить ICE №130-классы и FFI).
- Same-unit own-package qualified импорты и импорт корня пакета
  (`import mypkg.{x}`) не переписываются — граница названа в доке модуля
  (same-unit рёбер не требует; у корня нет последнего сегмента); измеренного
  носителя у этих форм нет.
- Ограничения мелкого текстового обхода в силе (import в несколько строк,
  embed-ассеты — оговорка №1889).
- `nova check std/src` до/после не гонялся — правка вне компилятора и std
  (см. std/src выше).
