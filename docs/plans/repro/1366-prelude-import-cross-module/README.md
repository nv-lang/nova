# №1366 — `import std.prelude.{Vec}` в одном модуле отнимает `println` у ДРУГОГО модуля

Найдено окном nova-postgres (Kim Code), 2026-09-28, в `src/pgnet/pool.nv`;
минимальная двухфайловая проба снята интегратором 2026-09-29.

Суффикс `.nv.txt` — улика вне раннера. Собрать пакет (`nova.toml`, `src/main.nv`,
`src/helper.nv` без `.txt`) вне репы и:

```sh
nova-cli/target/release/nova.exe build <каталог>/src/main.nv -o <куда-нибудь>.exe
```

| проба | `build` |
|---|---|
| `helper.nv` импортирует `std.prelude.{Vec}` | `undefined identifier \`println\`` — в `helper.nv:7` **и в `main.nv:6`** |
| контроль: в `helper.nv` вместо этого `std.collections.vec.{Vec}` | `undefined identifier \`println\`` только в `helper.nv:7`; `main.nv` чист |
| контроль: `helper.nv` без импортов | `undefined identifier \`println\`` в `helper.nv:4`; `main.nv` чист |

Две нижние строки показывают поведение НЕ главного модуля без явного импорта
`println` — оно одинаково в обоих контролях и к дефекту не относится (здесь не
судится, законно ли оно). Дефект — верхняя строка: импорт в одном модуле меняет
разрешение имён в другом.
