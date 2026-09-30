<!-- SPDX-License-Identifier: CC-BY-4.0 -->
# №761 — newtype в payload арма и порядок объявления

Замер 2026-09-30, облачная сессия, ветка `p761-newtype-payload-order`, база
`origin/integrate` (`affa009`). Все пробы — вне репозитория, во временном
пакете; здесь они лежат с суффиксом `.nv.txt`.

## Недостающий фактор

Порядок объявления ломает сборку, только когда payload арма **достаётся
связыванием образца и оборачивается в `Some`** (`ToRow(r) => Some(r)`).
Четыре встречные пробы интегратора были зелёными, потому что ни одна не брала
payload из связки арма: `Some(7 as Row)` типизируется по значению, а не по
хранимому типу payload.

Число пиров, транзитивный импорт и `Option[Row]` в полях записи роли не играют:
красная форма собирается из **одного файла** (проба `a`) и из двух пиров (`e`).

## Механизм

Представление newtype/alias в C (`Row` → `nova_int`) записывалось в
`type_aliases` только в `emit_type_decl`, то есть в порядке элементов модуля
(порядок объявлений в файле, а между пирами — отсортированный порядок файлов).
Сумма, объявленная ВЫШЕ newtype, опускала payload раньше этой записи и получала
умолчание кучевой записи `Nova_Row*`; связка арма наследовала этот тип;
`Option[Row]` в сигнатуре, опущенный позже, становился `NovaOpt_nova_int`, а
`Some(r)` из связки — `NovaOpt_Nova_Row_p`. Одна мономорфизация — два
представления.

## Пробы

Сборка из каталога пробы (с `nova.toml` `[lib] src = "."`, файл кладётся под
именем без `.txt`; для `e`/`f` — оба файла рядом, как пиры одного модуля):

```sh
NOVA_CACHE=0 NOVA_STD_PATH=<repo>/std NOVA_CG_INCLUDE=<repo>/compiler-codegen \
NOVA_RT_DIR=<repo>/compiler-codegen/nova_rt \
<repo>/nova-cli/target/release/nova build <вход>.nv -o probe.bin && ./probe.bin
```

| проба | ось | до фикса | после |
|---|---|---|---|
| `a_sum_above_some_binding` | один файл: сумма ВЫШЕ newtype, `Some(r)` из арма | ❌ 1 ошибка C: `assigning to 'NovaOpt_nova_int' from incompatible type 'NovaOpt_Nova_Row_p'` | ✅ `row=7` |
| `b_newtype_above_control` | то же, newtype ВЫШЕ суммы (контроль) | ✅ | ✅ |
| `c_bare_payload_no_option` | сумма выше, payload читается `r as int`, без `Option` | ✅ | ✅ |
| `d_fresh_some_not_binding` | сумма выше, `Some(7 as Row)` — не из связки | ✅ | ✅ |
| `e_peers_*` | пиры: сумма и потребитель в `a_sum.nv` (1-й), newtype в `z_row.nv` (последний) | ❌ та же 1 ошибка | ✅ |
| `f_peers_*` | пиры в обратном порядке: newtype в `a_row.nv` (1-й) | ✅ | ✅ |
| `g_alias_sum_above` | как `a`, но `type Row alias int` | ❌ 4 ошибки: `unknown type name 'Nova_Row'` ×3 и та же `NovaOpt` | ✅ |

`c` и `d` — это и есть граница: меняется один фактор (откуда берётся значение
под `Some`), и только связка арма краснеет.

## Носитель (Карина)

Перенос `ConstRow` в `sem/defs.nv` (к `ConstTable`), `FieldRow` и `VariantRow`
в `sem/sem.nv` (к `FieldDef`/`VariantDef`), самосборка
`nova build novac/src/main.nv`:

* до фикса — 4 ошибки C: `NovaOpt_Nova_ConstRow_p` против `NovaOpt_nova_int`
  ×2 (`CheckOut.const_of`, `DefTable.const_row` — оба `Const(row) => Some(row)`),
  `NovaOpt__NovaTuple_2_17_Nova_VariantRow_p_8_nova_int` и
  `..._15_Nova_FieldRow_p_...` против `NovaOpt__NovaTuple_2_8_nova_int_8_nova_int`
  (`variant_range_of`/`field_range_of` — `VariantRows(first, cnt) => Some((first, cnt))`);
* после — `built`.

Кортежная пара ошибок — тот же фактор, а не №1155: связка арма внутри кортежа
под `Some`.
