# REPORT-carina-mono-k2-7 — №1666 (ветка p274-carina-mono-k2)

**Мера 0.2: 11 → 11, ICE 0.** Синк с main не делал — №1666 взят по твоей поправке 00:41 на своей ветке.

## ИТОГ

| пункт | итог | коммит |
|---|---|---|
| №1666: ICE «a substituted type the checker never interned» на `Gbx.make("t", 2)` | закрыт | `6e6dc600d` |
| тот же класс с владельцем-СУММОЙ (`LinkedList.from_array([1, 2, 3])` из std) | закрыт | `6e6dc600d`, `127b504ca` |

## №1666

**Причина.** Статическая дверь спрашивала «чьё тело» (`interop.has(c_static_symbol(..))`) раньше, чем
интернировала сигнатуру экземпляра. `c_static_symbol` подставляет владельца (`Gbx[T]` → `Gbx[str]`) через
`subst_type`, а эта дверь интернер только ЧИТАЕТ. Терма ещё нет, отсюда ICE. Так падал любой подходящий
обобщённый статический вызов поданного типа. Два неподходящих вызова из клеток №1650 отказывали раньше и
до этого места не доходили — поэтому тогда не было видно.

**Починка.** `@intern_instance_sig` стоит до вопроса «чьё тело» (раньше — после записи вызываемого).

**Тот же класс, второй носитель.** С владельцем-суммой та же дверь падала следующим ICE: «the
record-shaped instance body of a generic SUM» (std, `LinkedList.from_array([1, 2, 3])`). Как оракул пишет
это имя:
- с turbofish (`LinkedList[int].from_array`) — `Nova_LinkedList____nova_int_static_from_array`, по
  экземпляру, как у записи;
- без turbofish — голое `Nova_LinkedList_static_from_array`. Это его стёртый запасной путь, и программа
  на нём падает segfault (находка 1 ниже).

Первый коммит (`6e6dc600d`) взял голое имя — я снял его с C оракула без turbofish. Это копия дефектного
пути; исправлено в `127b504ca`: сумма пишется по экземпляру. Текст `<Head>____<arg>` вынесен в
`applied_body_text`. `instance_body` сохраняет свою охрану записи — методы Option/Result идут через
семейное написание.

**Клетки** (`pipeline/static_args_test`):
- подходящий `Gbx.make("t", 2)` проверяется и эмитится (`compile_to_c`, ноль диагностик);
- поданная обобщённая сумма `Sx.wrap(3)` — без ICE.

**В обе стороны:**
- интернирование вернуть после записи → клетка Gbx падает ICE;
- ветку суммы выключить → клетка Sx падает ICE;
- восстановлено → модульные 10/10.

## НОВОЕ ДЛЯ РЕЕСТРА

1. **Оракул: обобщённый статический метод СУММЫ без turbofish компилируется в стёртый символ и падает
   при исполнении.**
   ```
   import std.collections.linkedlist.{LinkedList}
   fn main() {
       ro l = LinkedList.from_array([1, 2, 3])
       println(l.len())
   }
   ```
   Собирается, при запуске — segfault. С `LinkedList[int].from_array(..)` печатает `3`. Без `len()`
   печатает. Это тихий неверный код (K1) в Rust-компиляторе. Вызов — `Nova_LinkedList_static_from_array`
   вместо экземпляра: путь emit_c.rs ~23807 при невыведенном параметре уходит в стёртый запасной вариант.
2. **Карина: вариант НЕэкспортированной суммы чужого модуля виден в теле поданного экземпляра.** В
   экземпляре `LinkedList.from_array` строка `mut acc = Empty` отказывает: «this variant name is declared
   by more than one sum in scope». Второе объявление — приватный `type Slot[K, V] enum | Empty | ..` из
   `std/src/collections/hash_map/core.nv`, а linkedlist.nv ничего не импортирует. Оракул тело принимает.
   Класс — видимость вариантов поданных сумм без учёта экспорта/импорта модуля. Сейчас скрыто подмножеством:
   у этого тела дальше идёт `match` на применённой сумме, а его Карина не компилирует (E2-b2). Не чинил —
   не назначено.

## ПРОВЕРКИ

- Модульные тесты: 10 модулей (46 файлов), PASS 10.
- Сверка всех фикстур (267): каждая pos принята, ни одна neg не принята.
- Стражи: file-size, ice-messages, line-length, no-silent-skip, subset-debt-dated, surface (651, на
  базе), resolve-discipline, lint — ok.

## АГЕНТЫ

Не запускал.

## КОММИТЫ

```
127b504ca novac(#1666 follow-up): a generic sum's static is spelled per instance -- the oracle's bare name is its erased fallback
6e6dc600d novac(#1666): a generic static call that fits is interned before its symbol is spelled -- and a sum owner is spelled by name
```

## ОЧЕРЕДЬ

По твоему письму — после синка:
- `"abc".chars().count()` / `str @pad_start` на перегенерированной оболочке;
- имена полей из зарезервированных слов (D83).
