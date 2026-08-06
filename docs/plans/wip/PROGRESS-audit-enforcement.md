# Аудит «правило объявлено — энфорс есть?»

Окно p-audit-enforcement. Метод: минимальный файл с заведомо нарушенным правилом,
`nova check` (не `build`) на нём. Ничего не чинится — только инвентаризация.
Пробы лежат в `probes/pNN_*/main.nv` этого worktree (каждая — свой каталог/модуль/
уникальное имя типа). Бинарь: `nova-cli/target/release/nova.exe` из main-репы
(в этом worktree своей сборки нет). Для `nova build` — `NOVA_RT_DIR`/
`NOVA_CG_INCLUDE`, указывающие на main-репу (иначе FATAL: libuv submodule not
initialized; сам submodule — 468 МБ, копировать не стал).

Статус: пункт 1 периметра (бáунды дженериков) закрыт целиком. Пункты 2–5 — не начаты.

## Таблица: пункт 1 — бáунды дженериков

| механизм | где объявлен | проба | вердикт check | вердикт build | класс |
|---|---|---|---|---|---|
| `AsSlice[T]` (метод-бáунд `@append[S AsSlice[T]]`) | protocols.nv:477; mutate.nv:283 | `probes/p1_asslice`: `[]u8.new().append(NoSlice{junk:7})` | **ok** (ложный) | падает на этапе C (см. №381, уже зафиксировано ранее) | расхождение check/build |
| `Compare` (метод-бáунд, форма `@method[K Bound]`) | protocols.nv:103(Iter)/пример своей формы ниже | — (см. корень «method-level bound», ниже прямая проба p13) | не проверялось напрямую по Compare-методам, но корень тот же | — | входит в групповой корень |
| `Clone` (метод-бáунд `@combine[S Clone]`, синтетическая проба точно по форме `@append`) | probes/p13_methodbound | `Box13.combine(NoClone13{...})`, `NoClone13` без `@clone` | **ok** (ложный) | **built** (ложный) — рантайм: `other.clone()` скомпилировался в `return other;` (identity, никакого клона, тихая подмена семантики) | и check, и build молчат — дыра, не просто расхождение |
| `Equal` (default-body метод `@equal(other Self) -> bool => @compare(other) == 0`, протокол верхнего уровня, НЕ метод-бáунд) | protocols.nv:83-85 | `probes/p8_equal`: `fn[T Equal] use_equal(a T, b T) -> bool => a.equal(b)` на `NoEqual8` без `@equal`/`@compare` | **ok** (ложный) | **built** (ложный) — рантайм печатает `false`, НО генерируемый C: `a.equal(b)` → `Nova_HashMap_method_equal(a, (void*)(b))` — резолвер молча взял ЧУЖОЙ одноимённый метод (`HashMap.equal`) на несвязанном типе, `void*`-каст скрывает type confusion. UB, не просто «не то значение» | check и build молчат — дыра; отдельный от method-bound корень (default-body протокола) |
| `Hash` | protocols.nv:60-62 | `probes/p7_hash`: `fn[T Hash] use_hash(x T) -> u64` на `NoHash7` | **FAIL** (корректно) | — | энфорсится |
| `Clone` (top-level `fn[T Clone] name(...)`, НЕ метод-бáунд) | protocols.nv:135-137 | `probes/p3_clone` | **FAIL** (корректно) | — | энфорсится |
| `Display` | protocols.nv:314 | `probes/p4_display`: `"${v}"` на типе без `#impl(Display)` | **FAIL** (корректно, `E_INTERP_NO_DISPLAY`) | — | энфорсится (свой механизм — opt-in impl gate, не protocol-bound) |
| `Debug` | protocols.nv:342 | `probes/p5_debug`: `"${v:?}"` | **FAIL** (корректно, `E_DEBUG_PRINTABLE_NOT_IMPLEMENTED`) | — | энфорсится |
| `Write` (top-level `fn[T Write] name(...)`) | protocols.nv:165-167 | `probes/p6_write` | **FAIL** (корректно) | — | энфорсится |
| `Next[T]` (top-level) | collections.nv:87-89 | `probes/p9_next` | **FAIL** (корректно) | — | энфорсится |
| `Iter[I]` (top-level) | collections.nv:103-105 | `probes/p10_iter` | **FAIL** (корректно) | — | энфорсится |
| `Index[K,V]` (top-level) | protocols.nv:435-437 | `probes/p11_index` | **FAIL** (корректно) | — | энфорсится |
| `MutIndex[K,V]` (top-level) | protocols.nv:454-456 | `probes/p12_mutindex` | **FAIL** (корректно) | — | энфорсится |

## Групповые корни (пункт 1)

### Корень А — «method-level generic-parameter бáунд не проверяется вообще»
Форма: `fn ReceiverType[...] @method[S SomeBound](args) -> ...` — бáунд `SomeBound`
висит на типовом параметре, объявленном НА САМОМ МЕТОДЕ (`[S ...]` после `@method`),
а не на верхнеуровневой `fn[T Bound] freeFunc(...)`. Для этой формы `nova check`
**вообще не проверяет** удовлетворение бáунда на call-site — доказано дважды
независимо (AsSlice/№381 и синтетический Clone/p13, разные протоколы, разный
codegen-фоллбэк). Для формы `fn[T Bound] freeFunc(...)` (бáунд на функции/на
receiver-типе, не на методе) — проверка работает штатно (Hash/Clone/Write/Next/
Iter/Index/MutIndex все корректно словили нарушение).

Известные call-site'ы этой формы в `std/` (все — потенциальные дыры для
пользовательского кода, вызывающего эти методы с неподходящим типом):

- `std/src/collections/vec/access.nv:220` — `@binary_search_by_key[K Compare]`
- `std/src/collections/vec/mutate.nv:262` — `@extend[S Iter[T]]`
- `std/src/collections/vec/mutate.nv:283` — `@append[S AsSlice[T]]` (=№381)
- `std/src/collections/vec/sort.nv:232` — `@sort_by_key[K Compare]`
- `std/src/collections/vec/sort.nv:242` — `@sort_unstable_by_key[K Compare]`
- `std/src/collections/vec/sort.nv:287` — `@dedup_by_key[K Equal]`
- `std/src/encoding/serde/serde.nv:186,202,206,210,214,218,285,319,340` — `@serialize[S Serializer]` (протокольный метод + ×8 impl'ов)

Итого минимум **9 сигнатур** в std одной формы, плюс любой пользовательский код,
объявляющий метод с собственным `[S Bound]`. Один фикс в checker'е (научить его
проверять method-own type-param бáунды на call-site, не только function-level)
закрывает весь класс разом — это ОДИН дефект, не N.

**Кодоgen-поведение на непройденной проверке — само по себе разное и опасное:**
- `Clone` (p13): `other.clone()` без метода → `return other;` (тихий identity,
  никакой ошибки компиляции C).
- `AsSlice` (№381, ранее задокументировано): падает на этапе C (недостающий
  символ/несовместимость типов) — то есть здесь хотя бы РАСХОДИТСЯ check/build,
  а не тихо «работает».

Значит сам этот корень A ветвится на подпримеры с разным поведением на C-этапе —
не гарантированно «упадёт на build», иногда тихо скомпилируется в семантически
неверный код. Это делает корень A более опасным, чем формулировка №381
(«молчит в check, ловит build») — в общем случае может НЕ ловить и build.

### Корень Б — «протокол с default-body методом не проверяет ЗАВИСИМОСТИ default-тела»
`Equal.@equal` имеет default-тело `=> @compare(other) == 0`. Любой тип
структурно «удовлетворяет» `Equal` просто по наличию默认 метода в протоколе —
компилятор не проверяет, что `@compare` (от которого зависит default-тело)
реально существует у типа-аргумента. На call-site (`a.equal(b)`) это не падает
ни на check, ни на build — вместо этого codegen резолвит `@compare`/`@equal` в
**произвольный одноимённый метод где-то ещё в программе** (`Nova_HashMap_method_equal`
в пробе), с `void*`-кастом, глушащим типовое несоответствие. Это type confusion /
UB, не просто «неверный ответ». Затронут ровно один протокол в текущем
`protocols.nv` (`Equal` — единственный с default-body методом среди
Hash/Equal/Compare/Clone/Write/Fmt/Display/Debug/Index/MutIndex/AsSlice/Next/Iter),
но сам механизм («default-body метод протокола не type-checks на предмет
зависимости от других методов бáунда на call-site, а неразрешённый вызов метода
падает в произвольный одноимённый символ, а не в ошибку компиляции») —
самостоятельный, потенциально более широкий по будущим протоколам с
default-body (сейчас таких мало, но паттерн предупреждает: default-body в
протоколе — источник этого класса дыр всегда).

## Ранжирование (пункт 1, промежуточное — до финала по всем пунктам)

1. **Корень А (method-level bound не проверяется)** — закрывает ≥9 известных
   сигнатур std + весь пользовательский код такой формы одним фиксом в
   checker'е. Наивысший приоритет: самый широкий охват, уже дважды подтверждён
   независимыми пробами.
2. **Корень Б (default-body протокола не проверяет свои зависимости + резолвер
   падает в произвольный одноимённый символ вместо ошибки)** — уже сейчас
   type confusion/UB на реальном коде, а не просто "неверный результат".
   Меньше охват (1 протокол сегодня), но опаснее по последствиям —
   второй по приоритету, возможно первый по severity (не по охвату).

---

*(Пункты 2–5 периметра — в работе, будут дописаны в этом же файле.)*
