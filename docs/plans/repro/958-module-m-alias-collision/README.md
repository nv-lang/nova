# Проба: модуль пользователя по имени `m` сталкивается с внутренним алиасом std

Реестр 221.1, строка №958 (номер дал интегратор 2026-09-05; заведена окном 274 по его
требованию «проба-каталог, не README»). Найдена побочно при пробе №957.

## Что измерено (2026-09-05, `nova-cli/target/release/nova.exe`)

Каталог рядом: `main.nv` — `module probe760; import ./m.{used}; fn main() { println(used()) }`;
`m/m.nv` — `module probe760.m; export fn used() -> int => 1` (плюс невызываемая функция,
для этой пробы неважная). Пользовательский модуль называется `m`.

| команда | результат |
|---|---|
| `nova build main.nv -o probe.exe` | `std/src/collections/hash_map/core.nv:146:9: error: [E7401] no function `insert` in module `m`` |
| то же с модулем, переименованным в `helper` | `built` |

Ошибка указывает В STD: `hash_map/core.nv:146` зовёт что-то через алиас `m` (внутренний
импорт std), и резолвер отвечает на него ПОЛЬЗОВАТЕЛЬСКИМ модулем `m`, где `insert` нет.
То есть имя модуля пользователя утекает в пространство имён std-файла — либо алиасы
импортов std не изолированы от модулей программы, либо `import ./m` регистрирует `m`
глобально. Какое из двух — чинящему; проба даёт оба входа (`m/m.nv` и строка std).

## Воспроизвести

Файлы лежат как `.nv.txt`; скопировать в пустой каталог, сняв `.txt`:

```sh
P=target/probe958; mkdir -p $P/m
cp docs/plans/repro/958-module-m-alias-collision/main.nv.txt $P/main.nv
cp docs/plans/repro/958-module-m-alias-collision/m/m.nv.txt $P/m/m.nv
nova build $P/main.nv -o $P/probe.exe     # E7401 из std/src/collections/hash_map/core.nv:146
```
