# №1309 — возврат `ro`-параметра через не-`ro` результат: оракул против novac

Оракул отвергает возврат параметра, связанного `ro` (записан без `mut`, D176), через
результат без `ro`, если тип значения делится с источником: `mut w = f(v)` у
вызывающего писал бы то, что параметр заморозил (D246, `E_READONLY_COERCE`,
`spec/decisions/02-types.md`). novac до 2026-09-23 принимал это молча на всех
нескалярных типах. Найдено при приёмке пачки `nova-kim`: волна Э.4 засеяла
встроенный `Effect`, и фикстура `novac/fixtures/effect_e_name/neg_1.nv`, чья
программа как раз такова, осталась без единой диагностики.

Каждая проба — одна форма; `fn main() { println(1) }` в хвосте у всех.

Колонки: оракул (`nova check`), novac с `main`@`eed26c634` (до правки), novac
после правки двери `@judge_ro_param_return` (`novac/src/check/return_rules.nv`).
«other: …» — novac отвергает форму по ДРУГОЙ, честной причине подмножества;
это граница той формы, не этой двери.

| проба | оракул | novac до | novac после |
|---|---|---|---|
| `bool` | accepted | accepted | accepted |
| `chr` | accepted | accepted | accepted |
| `d_alias` | E_READONLY_COERCE | accepted | accepted |
| `d_consume` | accepted | accepted | accepted |
| `d_method` | E_READONLY_COERCE | accepted | E_READONLY_COERCE |
| `d_mut_param` | accepted | accepted | accepted |
| `d_pattern` | accepted | other: outside the subset: a `match` on an applied sum … | other: outside the subset: a `match` on an applied sum … |
| `d_recv` | accepted | accepted | accepted |
| `d_recv_mut` | accepted | accepted | accepted |
| `d_return_stmt` | E_READONLY_COERCE | accepted | E_READONLY_COERCE |
| `d_shadow` | accepted | other: outside the subset: the shell novac links into c… | other: outside the subset: the shell novac links into c… |
| `e_fresh_ro` | accepted | accepted | accepted |
| `e_shadow_mut` | accepted | accepted | accepted |
| `e_shadow_ro` | accepted | accepted | accepted |
| `eff` | E_READONLY_COERCE | other: outside the subset: novac does not know this typ… | E_READONLY_COERCE |
| `enumv` | E_READONLY_COERCE | accepted | E_READONLY_COERCE |
| `f64` | accepted | accepted | accepted |
| `i64` | accepted | accepted | accepted |
| `int` | accepted | accepted | accepted |
| `newt` | accepted | other: outside the E1 subset: field must declare a type… | other: outside the E1 subset: field must declare a type… |
| `opt` | E_READONLY_COERCE | accepted | E_READONLY_COERCE |
| `rec` | E_READONLY_COERCE | accepted | E_READONLY_COERCE |
| `str` | accepted | accepted | accepted |
| `sum` | E_READONLY_COERCE | accepted | E_READONLY_COERCE |
| `u8` | accepted | accepted | accepted |
| `vec` | E_READONLY_COERCE | accepted | E_READONLY_COERCE |
| `vrec` | accepted | accepted | accepted |

**Что читать в таблице.** Дверь зеркалит оракул и НЕ ШИРЕ: на каждой форме, которую
оракул отвергает, novac после правки тоже отвергает — кроме `d_alias`; на каждой,
которую оракул принимает, новая дверь отказа не даёт.

**Названный остаток №1309.** `d_alias` (`ro y = x` затем `y`) оракул отвергает, а
дверь его не видит: нужна передача «источник `ro`» через привязку, и это работа окна
Карины. Не мерены: newtype над нескалярным представлением и кортеж (кортежи в
параметре novac и так отвергает по имени). Вид типа, добавленный позже, дверь
ПРИНИМАЕТ до своего замера — это сказано в её комментарии.

Снять заново (из корня главного дерева; `novac check` иначе не находит std):

```sh
for f in docs/plans/repro/1309-ro-param-return/*.nv.txt; do
  cp "$f" /d/Temp/p.nv
  echo "$(basename "$f") oracle=$(nova-cli/target/release/nova.exe check /d/Temp/p.nv 2>&1 | grep -oc E_READONLY_COERCE)" \
       "novac=$(novac/target/novac.exe check /d/Temp/p.nv 2>&1 | grep -oE '"message":"[^"]{0,40}' | head -1)"
done
```

Замер 2026-09-23 (интегратор). Строка реестра — №1309.
