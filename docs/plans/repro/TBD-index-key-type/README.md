<!-- SPDX-License-Identifier: CC-BY-4.0 -->
# Проба: ключ индекса не `int` — оракул принимает (строка реестра №TBD, заводит интегратор)

## Находка

Заведено 2026-10-02 облачной сессией `p274-carina-cast` по слову интегратора.
Оракул собирает и исполняет индекс `xs[i]`, у которого ключ НЕ `int`:

- `cell_u8.nv` — ключ типа `u8` над обычным `Vec[int]` (локальная переменная);
- `cell_newtype.nv` — ключ newtype над `int` (форма `NodeId` Карины) над `Vec[int]` в поле записи.

Обе клетки у оракула печатают `20` (`*.out` рядом — вывод оракула и Карины).

## Норма

Прямой фразы «индекс — только `int`» в спеке нет, но вывод однозначен (решение интегратора
2026-10-02):

- D238 (`spec/decisions/03-syntax.md`, около строк 11351–11385): `Vec[T] @index` принимает
  ключ `int` или `Range`; std так и объявляет: `export fn Vec[T] @index(i int) -> T`
  (`std/src/collections/vec/access.nv:30`);
- D405 (`02-types.md`, около строки 17693): неявной числовой конверсии нет нигде;
- D52 (`02-types.md`, около строки 435): newtype над `int` — не `int`.

Литерал-индекс `xs[0]` к этому не относится: он берёт тип позиции (D489).
Типизированные индексы (`IndexVec`, `Ordinal`) — отдельная работа, план 281 (D470 §6).

## Связь с прошлым

Строка №894 («сахар `a[k]` не проверяет тип ключа») закрыта 2026-09-05. Эта проба показывает, что на
дереве `origin/integ/merge-train-1001` (e2f4fc96) ключ `u8` и ключ-newtype оракулом по-прежнему
принимаются — регресс либо дыра, которую тот фикс не покрыл. Разбор — за тем, кто возьмёт строку.

## Карина

Отказывает на обеих клетках: `E_NOVAC_LANG`, «an index is an `int` (D238 ...) -- there is no implicit
numeric conversion (D405), and a newtype over `int` is not an `int` (D52); convert with `as int`».
До 2026-10-02 тот же отказ шёл под кодом подмножества с текстом «the language allows other integer
types here» — это был замер оракула, а не спеки; поправлено той же волной (`check/messages.nv`
INT_INDEX_MSG). Фикстуры: `novac/fixtures/index_key/{pos_1,neg_1}.nv`.

## Как воспроизвести

```sh
nova-cli/target/release/nova build docs/plans/repro/TBD-index-key-type/cell_u8.nv -o /tmp/c && /tmp/c
nova-cli/target/release/nova build docs/plans/repro/TBD-index-key-type/cell_newtype.nv -o /tmp/c && /tmp/c
novac/target/novac check docs/plans/repro/TBD-index-key-type/cell_u8.nv
```
