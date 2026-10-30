# Отчёт задачи 128 — проверка релизного build Carina 0.3: 118 примеров и 9 пакетов

Замер критерия [274.16](../../../docs/plans/274.16-carina-all-examples-and-packages.md)
(Ш.5/Ш.7, Windows + Clang, pinned B1) на свежем `origin/main`. Задача —
измерение: ничего не чинилось, Oracle не собирался и не использовался.
Улики (таблицы построчно и дословные логи всех 137 запусков) —
[evidence/](evidence/README.md).

## РЕПРО

Контур (предусловие: `git submodule update --init` в свежем worktree, иначе
честная диагностика «required Nova runtime input is missing:
…/libuv/include/uv.h»; далее rt-архив собирается один раз и кэшируется):

```
cd nova-driver && cargo build --release          # драйвер из ЭТОГО дерева
python scratch-opencode/task128/run-118.py <list-118> <outdir>   # 118 примеров
python scratch-opencode/task128/run-packages.py                  # 9 пакетов
```

Каждый пример: `nova.exe build <файл> -o <bin> --toolchain clang`, при rc=0 —
запуск бинаря. Пакетные входы — через минимальный install-контур
(`NOVA_RT_DIR` → родитель с driver-ассетами, `NOVA_CG_INCLUDE` →
`compiler-codegen` этого дерева); `package-release.ps1` не применялся — он
требует `nova-compat.exe`, то есть Oracle-бинарь, что вне границ задачи.
Оба прогона — через очередь машины (watch `1793370328486-obswkp` 23 мин,
`1793371050432-y0zmip` 1 мин).

## ТОЧКА (что именно измерено — источники и бинарь разведены)

- Исходники: `git rev-parse HEAD` = `a5de9ad360b12a3935c04dcb7025c532ae240898`
  (ветка `task/128-proverit-reliznyy-build-carina-0-3-118-p`),
  база `origin/main` = `d6d0269477fdb6897a7bc9d9ff569752029e6236`.
- Бинарь драйвера `nova-driver/target/release/nova.exe`: SHA-256
  `f7cb5f18cc79ba75c8cbcfa4e38783ea79738766c412aff43b146a94e250ea2c`
  (64 hex — хэш ФАЙЛА, не commit SHA).
- Сид: в каждом из 137 запусков `carina-gate verify-seed PASS` (строка есть в
  каждом логе): ZIP `7e8264fd…69cdb`, B1 exe `824bf597…6996f4`, source
  `3729abcc…21d5` — совпали с пинами `nova-driver/src/carina_build.rs:19-23`.

## РЕЗУЛЬТАТ 118 примеров (`evidence/table-118.tsv`)

| Исход | Строк |
|---|---|
| **PASS (build + run)** | **62** |
| FAIL — emit `E_NOVAC_SUBSET` (горизонты подмножества 274.16) | 48 |
| FAIL — emit `E_NOVAC_LANG` (сид ссылается на D54/D406, корпус Oracle-эпохи) | 3 |
| FAIL — emit `E_NOVAC_ICE` (`program.nv:157`) | 1 |
| FAIL — link `lld-link: undefined symbol: nova_fn_main_impl` (у файла нет `fn main`) | 4 |
| **Итого FAIL** | **56** |

Из 62 PASS: 60 прогонов rc=0, 2 — rc=101 (`basics/neg_overflow_panics.nv`,
`basics/requires_violation_names_line.nv` — намеренные panic по имени и по
stderr: `panic: negation overflow`, `requires failed`). Время build PASS:
медиана 12,0 с, максимум 47,4 с.

Кросс-чек с фактурой задачи 119 (её таблица утеряна, известны агрегаты):
62 PASS, 60+2 run, 52 emit-стадия + 4 link-no-main — **совпало точно**
(52 = 48 subset + 3 lang + 1 ice; 119 сворачивала все emit-отказы в одну
группу).

Замечания по составу FAIL:

- Все 62 PASS — однофайловые программы; из 18 входов, прошедших стадирование
  программы (№1889), PASS нет: 15 subset (aggregator 9, http_proxy_chain 6),
  2 link-no-main (модули `tour/greeter` без `fn main`), 1 ICE.
- 4 link-no-main — корректный исход для не-входов (проверено: `fn main` в
  файлах отсутствует).
- Фикс №1922 жив: `getting_started.nv` FAIL с полной читаемой
  `E_NOVAC_SUBSET`-диагностикой в выводе (не пустой отказ;
  `evidence/logs-118/L092.txt`).

## РЕЗУЛЬТАТ 9 пакетов (`evidence/packages.tsv`)

HEAD | тег (рабочие копии на этой машине): bignum `3d865a055369` v0.1.0 ·
compress `f11c523ecc68` v0.1.3 · duckdb `a59a07328030` v0.2.2 ·
http `5505f31c4ef8` v0.2.1 · polaris `24f9d1757868` v0.2.4 ·
postgres `8a46520269cf` v0.1.0 · sdl `e636ac9286df` (без тега, master) ·
socks `9d44e5d5353e` v0.1.3 · tls `01d323b6423d` v0.2.1.

| Исход | Строк |
|---|---|
| FAIL — emit `E_NOVAC_SUBSET` («no such method in the declarations handed to novac» — внешние исходники сиду не переданы, блокер из задачи 122) | 18 (duckdb 3, polaris 12, postgres 3) |
| FAIL — lock-sync: `git clone https://github.com/nv-lang/nova-sdl` → remote not found (репозиторий не опубликован; до emit не дошло; среда, не продукт) | 1 (sdl `probes/thread-affine`) |
| **НЕ ПРОВЕРЕНО** — исполняемого входа нет; README предписывает `nova test src` / `scripts/gate.ps1` (объём 0.4) | 5 (bignum, compress, http, socks, tls) |

**PASS среди пакетов нет и не обещался:** критерий «9 пакетов» в текущем
pinned B1 упирается в названный блокер внешних зависимостей; нормативный вход
`nova build` для библиотечного пакета источниками не определён — отмечено НЕ
ПРОВЕРЕНО с причиной, критерий не выдумывался (указание интегратора 17:15).

## ДЕФЕКТЫ

- **Новых дефектов контура не выявлено.** Единственный ICE прогона
  (`tour/modules_tour.nv`, `E_NOVAC_ICE program.nv:157`: функция модуля
  `tour.greeter` «не достижима импортами entry») — **носитель известного
  класса №1924** (package-qualified импорт собственного пакета,
  `import nova_examples.tour.greeter.{…}`; относительные импорты `./…` в том
  же прогоне ICE не дают — совпадает с зелёным контролем №1924). Маршрут
  №1924 — новая сборка сида; дубль не заведён.
- Заведена запись **№1929** в `docs/plans/221.1-bug-sweep.md`: три
  регрессионных примера `aggregator/regressions/` отвергнуты как
  `E_NOVAC_LANG` со ссылкой сида на D54 (`() as int` ×2) и D406/№964 (дубль
  имени варианта ×1); кто прав — сид или корпус — не судилось, приоритет
  источников не нарушен. Номер присвоен интегратором 2026-10-30;
  «блокирует тег: ДА» — неразрешённое расхождение в обязательном релизном
  корпусе (не вердикт о правоте сторон).
- Неинициализированные подмодули свежего worktree — предусловие контура
  (письмо интегратора 17:38), дефектом не считается.

## ЧТО НЕ СДЕЛАНО

- `nova test` / package gates спутников — объём 0.4, вне задачи.
- Точный состав «9 пакетов» как нормативной команды 274.16 не восстановлен
  за пределами перечисленных входов; зафиксированы фактические исходы явных
  команд.
- Тайминги 17:37–17:38 (минута пересечения пакетного прогона со sweep'ем)
  могут быть слегка завышены; на статусы не влияет.

## ВЕТКА/КОММИТ

`task/128-proverit-reliznyy-build-carina-0-3-118-p`, коммит с этим отчётом и
уликами (SHA — в сдаче задачи).
