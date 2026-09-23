# №1308 — литерал аргументом образца-конструктора не читается парсером novac

Шесть проб ОДНОЙ формы; различается только аргумент `Some(...)` во второй строке:

```nova
fn classify(c Option[T]) -> int => match c {
    Some(<образец>) => 1
    Some(_) => 2
    None => 0
}
```

| файл | образец | оракул `nova check` | novac: отказов | первый отказ novac |
|---|---|---|---|---|
| `v_a.nv.txt` | `'a'` | 0 | 4 | безымянный откат на `)` |
| `v_nl.nv.txt` | `'\n'` | 0 | 4 | безымянный откат на `)` |
| `v_hash.nv.txt` | `'#'` | 0 | 4 | безымянный откат на `)` |
| `v_brack.nv.txt` | `'['` | 0 | 4 | безымянный откат на `)` |
| `ctl_int.nv.txt` | `7` | 0 | 4 | безымянный откат на `)` |
| `ctl_name.nv.txt` | `x` (привязка) | 0 | 2 | ИМЕНОВАННЫЙ, о границе `match` по прикладной сумме |

`ctl_int` — не контроль, а РАСШИРЕНИЕ класса: целый литерал падает так же, как символьный.
Настоящий контроль — `ctl_name`: без литерала парсерного отката нет.

Снять заново (из корня репозитория — `novac check` иначе не находит std):

```sh
for f in docs/plans/repro/1308-literal-in-ctor-pattern/*.nv.txt; do
  cp "$f" /d/Temp/p.nv
  echo "$f oracle=$(nova-cli/target/release/nova.exe check /d/Temp/p.nv 2>&1 | grep -ci error)" \
       "novac=$(novac/target/novac.exe check /d/Temp/p.nv 2>&1 | grep -c '"severity":"error"')"
done
```

Замер 2026-09-23, `main`@`497119774` (интегратор). Строка реестра — №1308.
