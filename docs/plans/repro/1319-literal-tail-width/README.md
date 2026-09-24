# №1319 — литерал в ветви значения-`if` не подгоняется к объявленному возврату

У ОДНОГО корня ТРИ разных текста отказа. Фикс, закрывший один текст, не закрыл
класс: приёмка — все три пробы проходят, а контроль остаётся чистым.

| файл | форма | оракул `nova check` | novac: отказов | текст отказа novac |
|---|---|---|---|---|
| `u64_branch.nv.txt` | `-> u64 => if c { 4 } else { 5 }` | PASS | 1 | `cannot return value of type int from a function declared -> u64 -- implicit int narrowing loses range` (`E_IMPLICIT_NARROWING`) |
| `i64_branch.nv.txt` | `-> i64 => if c { 29 } else { 28 }` | PASS | 1 | `cannot return value of type int from a function declared -> i64 (E7301): no implicit conversion between named types` |
| `mixed_branch.nv.txt` | `-> u8 { if c { x } else { 0 } }`, `x u8` | PASS | 1 | `the branches of a tail if must agree on a type (E2-b)` |
| `ctl_int.nv.txt` | `-> int => if c { 29 } else { 28 }` | PASS | 0 | — |

Почему текстов три, а корень один. Литерал в ветви остаётся `int`: подгонка
литерала к объявленному возврату (D227 п.2) спрашивает только голый литерал
хвоста. Дальше всё зависит от того, с чем этот `int` встретится:
- с `u64` — это сужение;
- с `i64` — это разные именованные типы: сужения нет, ширина и знак те же;
- с соседней ветвью `u8` — это несогласие ветвей, и до возврата дело не доходит.

Контроль с `-> int` доказывает, что дело в литерале против объявленного типа, а
не в `if` как значении.

Носители в дереве на 2026-09-23 (`main`@`8f0116f41`), оба сейчас ЗАМАСКИРОВАНЫ
более ранним именованным отказом:
- `novac/src/lex/lex.nv:327` `byte_at` — форма `mixed_branch`; раньше отказывает
  индексация `b[k]` (E2-b);
- `std/src/time/civil/date.nv:80` `days_in_month` — форма `i64_branch`; раньше
  отказывают `else if` как значение и вложенный `if` в ветви.

Снять заново (из корня репозитория — `novac check` иначе не находит std):

```sh
for f in docs/plans/repro/1319-literal-tail-width/*.nv.txt; do
  cp "$f" "${TMPDIR:-/tmp}/p1319.nv"
  echo "$f oracle=$(nova-cli/target/release/nova.exe check "${TMPDIR:-/tmp}/p1319.nv" 2>&1 | grep -c 'error:')" \
       "novac=$(novac/target/novac.exe check "${TMPDIR:-/tmp}/p1319.nv" 2>&1 | grep -c '"severity":"error"')"
done
```

Замер 2026-09-23, помощник (`nova-93`), novac собран из `main`@`8f0116f41`.
