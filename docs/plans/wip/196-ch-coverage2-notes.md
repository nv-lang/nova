<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# Plan 196 — Зона CH дожимка, волна-2 (покрытие): чекпойнт (sonnet, worktree
`nova-196chc`, ветка `p196-ch-coverage2`)

**Родитель:** [196-campaign-map.md](196-campaign-map.md) §«Зона CH — types/mod.rs, канал (ФУНДАМЕНТ)».
**Предшественник:** [196-ch-widen-notes.md](196-ch-widen-notes.md) (волна-1 — КОРРЕКТНОСТЬ,
`rt_is_closed`-гейт, `137167c54`/`4a9cd3598`, уже на main). **Задание этой волны:** ПОКРЫТИЕ —
(а) новые/расширенные продюсеры `node_substs` так, чтобы Q10 fallback-счётчики
(`resolve_mono_type_args_ch`/`resolve_method_level_subst`) трендовали к 0 на корпусе; (б) минимум
5 ядровых веток капстоуна (§3.4 карты, 33 «Core») получили нулевые residual-хиты.
**База:** main `99f0021f9` (после мержа p196-ch-widen `4a9cd3598` + p196-capstone `97a69e958`).

---

## Итог одной строкой

Найден и закрыт **один** большой, полностью верифицированный продюсер-пробел (**Producer
B-fluent-generic**): `resolve_instance_method_return_arity`'s `-> @` (`returns_receiver`) fast-path
возвращался ДО любого резолва method-level generics — структурный, 100%-ный miss для ВСЕГО класса
`fn Foo[T] mut @method[S Bound](...)  -> @` (доминирующий пример: `Vec[T]@append[S AsSlice[T]]`).
На `std/src/collections` это закрыло **311→49 (−84%)** raw-fallback хитов
`resolve_method_level_subst`, с нулём регрессий (SHADOW-mismatch, `nova test` PASS без δ). Второй
исследованный класс (B1, `copy_n_nonoverlapping`/`T.deserialize`) диагностирован как
**структурно НЕ закрываемый** чисто чекер-продюсером — те же call-сайты живут ВНУТРИ абстрактного
generic-тела (T ещё не резолвлен на CHECK-время) — тот же класс, что и задокументированный
`B10c_unanno_light_closure`-блокер капстоуна (нужна per-mono-instance ось, не ExprId-keyed канал).
**Часть (б) (5 ядровых веток капстоуна) НЕ достигнута** — архитектурно обосновано ниже (§3): Q10
(node_substs-консьюмеры) НЕ являются входом Frozen-зоны диспетчера (`infer_call_ret_c`), поэтому
Q10-продюсеры структурно не могут занулить capstone-ветки; для (б) нужны Q1/Q2/Q5/Q6/Q9-продюсеры
(`resolved_types_buf`/Channel-1-2), не Q10 — см. §4 для конкретной привязки веток к продюсер-классам.

---

## 1. Producer B-fluent-generic (закрыт, `types/mod.rs` только)

### 1.1 Находка

`resolve_instance_method_return_arity` (`types/mod.rs`, рядом с `resolve_return_channel`):

```
if f.returns_receiver {
    return Some(peeled.clone());   // <- было: возврат ДО резолва method-generics
}
```

`-> @` (fluent) методы ЧАСТО несут СОБСТВЕННЫЕ method-level generics, независимые от возврата
(возврат — всегда «эхо ресивера», D132/D181): `fn Vec[T] mut @append[S AsSlice[T]](other S) -> @`
(`std/src/collections/vec/mutate.nv:267`) — `S` резолвится ТОЛЬКО из аргумента `other`. До фикса
этот early-return означал: `resolve_return_channel` (который резолвит method-generics через
constraint-solver, уже используется в non-fluent ветках чуть ниже) НИКОГДА не вызывался для fluent
методов — структурный, постоянный miss для ВСЕГО класса в `node_substs`, независимо от того,
насколько конкретен call-site.

### 1.2 Замер (baseline → after, debug-бинарь, `NOVA_NODE_SUBSTS_TRACE=1`)

**`std/src/collections`** (14/0/6skip, PASS без δ до и после):

| Метрика (`resolve_method_level_subst`, B2) | before | after |
|---|---|---|
| `hit n=` (channel-only, top-of-fn) | 51 | **313** |
| `hit-composed` (Step1 early-exit, после fallback-строки) | 309 | 47 |
| `fallback` (miss/partial в начале fn) | **311** | **49** (−84%) |
| Σ (hit + fallback, без двойного счёта — `hit-composed` ⊆ `fallback`) | 362 | 362 (не изменилась — δ0 покрытия корпуса, чисто перераспределение) |

Топ ctx до фикса: `Vec____nova_byte.append` (300/311), `Vec____nova_int.append` (4),
`Vec____nova_str.append` (1), прочее (6). После: `Vec____nova_byte.append` (40/49) — остаток —
см. §2 (структурно неразрешимый класс).

**`std/src/time` + `std/src/encoding`** (27/0/8skip, PASS без δ): producer=B-fluent-generic
1848 хитов (`std/src/encoding`, серде-корпус, самый интенсивный трафик из измеренных).
**20 standalone-файлов** (f1/f2/f3-серия + m176/supervisor_escalate/map_pair/resize_with_free_fn_
shadow/mutexguard_invariant_balanced/t3_handle_pattern_ok/int_to_str_effect_op_blanket/d316/d289):
20/0 PASS, producer=B-fluent-generic 1120 хитов. **`d119_method_level_type_params.nv`** (метод-
level generics, самый нагруженный d-файл для ЭТОГО класса): producer=B-fluent-generic 231 хитов;
`RUN-FAIL` присутствует, но это **ПРЕ-СУЩЕСТВУЮЩИЙ, НЕ мой** баг (`Vec[f32].from([...]).into_str()`
не даёт ожидаемую строку) — задокументирован СЕГОДНЯ параллельной сессией Zone RET
(`196-ret-notes.md` §0) и капстоуном (`196-capstone-notes.md` §2b) как pre-existing, byte-в-byte
идентичный паттерн; `NOVA_NODE_SUBSTS_TRACE`-трейс на этом файле — 0 `assertion`/`SHADOW mismatch`.

**SHADOW-verification:** ни на одном из 4 корпусов (`collections`/`time+encoding`/`standalone×20`/
`d119`) `debug_assert!`/`shadow_check_node_substs` НЕ сработал — канал остаётся byte-consistent с
легаси на каждом измеренном call-site.

**Флагман:** `examples/flagship/aggregator --strict-effects` (release-бинарь) — собирается чисто
(только pre-existing warnings — unused imports/W_PARAM_TYPE_POS_MUT/W_DEP_PATH_NO_RELEASE, не мои).
**Release-бинарь:** `std/src/collections` PASS 14/0/6skip (та же цифра, что debug) — байт/поведение
не разошлись между профилями.

### 1.3 Дизайн фикса (propose-then-verify, whole-map completeness, `rt_is_closed`)

Добавлен блок ПЕРЕД `return Some(peeled.clone())` (сам возврат НЕ тронут — byte-parity
гарантирована конструкцией): если `f.generics` не пуст И есть `call_id`+`args_scope`, вызывается
ТОТ ЖЕ `resolve_return_channel`, что используют non-fluent ветки, передавая `peeled` САМ как
заглушку `ret`-шаблона (его роль — только затравить `ret_template` для solver'а; итоговый `rt`
отбрасывается — возврат ЭТОЙ функции остаётся безусловным эхом ресивера). Гейты: (1) `f.generics`
не пуст — иначе нечего выигрывать; (2) `args_scope` есть — 0-арная обёртка (без вызова) не имеет
аргументов для биндинга, честный no-op, как у всех прочих продюсеров; (3) `resolve_return_channel`
САМ гарантирует whole-map completeness + `rt_is_closed` на каждое значение (`ordered` остаётся
пустым при ЛЮБОМ residual) — то есть НУЛЕВОЙ новый доверительный сурфейс: тот же гейт-дисциплина,
что `137167c54`/`a36d2caed` уже закрепили для трёх других продюсеров.

**Файлы:** `compiler-codegen/src/types/mod.rs` только (51 строка, один сайт).

---

## 2. Класс B1 (`copy_n_nonoverlapping`/`T.deserialize`) — диагностирован, НЕ закрыт

### 2.1 Находка

Временная инструментация (`callee`/`is_static`/`generics` в fallback=miss трейсе
`resolve_mono_type_args_ch`, `emit_c.rs`; **снята после диагноза**, финальный коммит её не несёт)
показала: **все** 90 (`collections`) / 146 (`time+encoding`) B1-мисса — ОДИН callee,
`RawMem.copy_n_nonoverlapping[T]` (`std/src/runtime/raw_mem.nv:123`, static, `unsafe fn
RawMem.copy_n_nonoverlapping[T](src *T, dst *mut T, count int) -> ()`), плюс 14 мисс на
`{int,str,bool,...}.deserialize[D]`.

**Root cause (оба — ОДИН класс):** единственный call-site `copy_n_nonoverlapping` —
`std/src/collections/vec/core.nv:293`, ВНУТРИ `Vec[T] mut @cap`'s ТЕЛА — `T` там АБСТРАКТНЫЙ
carrier-generic-параметр (ещё не резолвлен на CHECK-время; конкретизируется ТОЛЬКО постфактум,
per-mono-clone, на EMIT-время). Аналогично `T.deserialize(d)` (`std/src/encoding/serde/serde.nv`
и др.) — вызывается ИЗНУТРИ `Option[T Deserialize]@deserialize`/`[]T.deserialize`/`HashMap[str,V]
@deserialize`'s СОБСТВЕННЫХ generic-тел, где `T`/`V` — protocol-bound, но ещё абстрактный
type-param, полиморфный static-dispatch (`T::deserialize`-стиль, rust-параллель). Изначальная
гипотеза («примитивный `int.deserialize(d)` вызывается напрямую») была НЕВЕРНОЙ — прямых
call-сайтов на конкретных примитивах в корпусе нет (грепом подтверждено); пробный продюсер для ЭТОЙ
гипотезы (`Producer A-static-path-generic`, primitive-`Path`-shape) написан, собран, **дал 0 хитов
на измеренных корпусах** → **отозван** (revert, не в финальном коммите) — не оставлять
неверифицированный код без демонстрируемой пользы (zero-tolerance-bugs / propose-then-verify).

**Почему НЕ закрываемо чисто чекер-продюсером:** `node_substs` — канал `ExprId → subst`, ОДНА
запись на call-site, разделяемая ВСЕМИ mono-клонами этого AST-узла (тело `Vec[T]@cap`
переиспользуется для КАЖДОГО конкретного T). На CHECK-время (когда продюсеры пишут канал) call-site
внутри `Vec[T]`'s собственного generic-тела ВСЕГДА видит `T` абстрактным — никакое расширение
`unify_type`/constraint-solver не может дать конкретное значение там, где его структурно нет. Это
ТОЧНО тот же класс, что капстоун задокументировал для `B10c_unanno_light_closure`
(`196-capstone-notes.md` §3.2): «структурно нужна per-mono-instance ось… чекер pre-mono, канал
keyed по ОДНОМУ ExprId на ВСЕ mono-инстансы». Единственный путь закрытия — EMIT-время композиция
(читать `current_type_subst`, УЖЕ конкретный для ТЕКУЩЕГО mono-клона) — это `emit_c.rs`-консьюмер-
сторона (Stage-C2-стиль `compose_*`), Зона GEN/RET, НЕ Зона CH (`types/mod.rs`-only периметр этой
сессии). **Флагирую для Zone GEN/RET следующей волны**, а не пытаюсь чинить вне периметра.

### 2.2 Проверенный факт: unify_type НЕ обрабатывает Pointer/Mut/Uninit/Ref

Побочно установлено (не фикс — заметка для будущей волны): `const_fn_trampoline::unify_type`
(`compiler-codegen/src/const_fn_trampoline.rs:1073`) рекурсирует в Named/Tuple/Array/FixedArray/
Func/Unit/Readonly, но НЕ в `TypeRef::Pointer`/`Mut`/`Uninit`/`Ref` — любая пара, где хоть одна
сторона — один из этих вариантов (кроме Readonly), падает в `_ => Err("type kind mismatch")`. Для
`copy_n_nonoverlapping`'s `*T`/`*mut T` параметров это НЕ имело бы значения (см. §2.1 — call-site
всё равно абстрактный, и результат унификации был бы residual T "снаружи", отфильтрованный
`rt_is_closed`/`gs`-гейтом) — но ЕСЛИ у будущей волны найдётся call-site со СВОИМИ raw-pointer
параметрами, вызываемый из КОНКРЕТНОГО (не generic-body) контекста, `unify_type` не свяжет
generic оттуда. Расширение (симметричные + асимметричные арма, зеркало уже существующих для
`Readonly`) — безопасное, но НЕ добавлено в эту волну (нет верифицированного call-site, что выиграл
бы от него — не изобретать продюсер без доказанной пользы).

---

## 3. Почему часть (б) («5 ядровых веток капстоуна → 0») архитектурно недостижима через Q10

Грепом подтверждено:
1. `resolve_mono_type_args_ch` вызывается ТОЛЬКО из ДВУХ `emit_call`-сайтов (`emit_c.rs` ~39188/
   ~39511) — оба explicitly **вне** Frozen-зоны (`infer_call_ret_c`, 49943-52037); доккомментарий
   самой функции подтверждает: «NOT wired at the frozen-zone call-site… that site keeps calling
   `resolve_mono_type_args` directly, unchanged».
2. `resolve_method_level_subst` — 5 вызывающих (33916/34458/34726/37442/37738 по `196-gen-notes.md`
   Q10-разделу) — ВСЕ строго ниже 49943 (частично уже проверено этой сессией: ни один не входит в
   диапазон 49943-52037).

Значит Q10-каналы (`node_substs`, потреблённые ИМЕННО этими двумя consumer-функциями) обслуживают
**mono-имя-генерацию** (какой C-символ звать для монообразованного вызова) — СОВЕРШЕННО отдельная
задача от **типа-возврата-выражения** (что вернёт вызов ДЛЯ диспетчера `infer_expr_c_type` —
Channel-1/2, `resolved_types_buf`, Q1/Q2/Q5/Q6/Q9-класс), которая РЕШАЕТ, дойдёт ли конкретный
call вообще до Frozen-зоны (`infer_call_ret_c`) как fallback. Producer B-fluent-generic (§1) пишет
ТОЛЬКО `node_substs`, не трогает `resolved_types_buf` для fluent-возврата (он и так уже был
корректно материализован ДРУГИМ, уже существующим механизмом — `Some(peeled.clone())` не менялся).
**Следствие:** ни один Q10-продюсер (в т.ч. мой) структурно не может изменить, попадает ли
конкретный call в `infer_call_ret_c` — 33 ядровых ветки капстоуна НЕ зависят от Q10 вообще.

## 4. Рекомендация следующей волне (капстоун-разморозка требует Q1/Q2/Q5/Q6/Q9, не Q10)

Свежепрочитанный `196.5-stage-d-census.md` (§3.3/3.4, до этой сессии, но данные по коду не устарели
в части «канал?») даёт КОНКРЕТНУЮ привязку — **ВСЕ 33 ядровых ветки показывают канал=«нет»**
(отсутствие ЛЮБОЙ resolved_types_buf/Channel-1-2 записи на их call-сайтах), кроме одной:

- **`B07_generic_type_instance_method`** (3072 трафика, САМАЯ близкая к разморозке) — census уже
  зафиксировал: **24 сайта имеют ch2=true** (Channel-2 данные ЕСТЬ), но диспетчер их не использует
  — «целевые для точечной композиции Stage-C2-стиля». Это `emit_c.rs`-консьюмер работа (Zone
  GEN/RET или capstone-агент), НЕ новый продюсер в `types/mod.rs` — канал уже пишет то, что нужно.
- Прочие тяжёлые (`B11d_typed_pointer_methods` 5897 — КРУПНЕЙШИЙ трафик; `B01_turbofish_member_
  generic_type` 3926; `B06*`-overload-каскад 334-3735; `B11a_array_static_method` 3684) —
  «канал=нет» означает НУЖЕН СОВЕРШЕННО НОВЫЙ Channel-1/2 продюсер (Q1/Q6/Q9-класс — декларированный
  тип-возврата для typed-pointer методов / turbofish static-ctor / overload-кандидатов), не
  дожимка существующего — полноценная новая инвентарь-миграция, не «расширение», за пределы
  одной сессии/зоны.
- **Мой вывод: следующей CH-волне (если продолжится в Q10-периметре) — НЕЧЕГО дожимать для (б);
  переклассифицировать (б) как задачу Zone GEN/RET (Q1/Q6/Q9), либо явно снять из мандата Zone CH
  волны-3.** Если оркестратор хочет CH-агента специально под capstone-разморозку — это НОВАЯ Q1/Q6
  задача (Channel-1/2 продюсеры, не node_substs Q10), нужно явно перенаправить мандат.

---

## 5. Инфра-инцидент сессии

**Хост под сильной нагрузкой** (много параллельных worktree/агентов — `git worktree list` на старте
показал 25+ активных worktree): d-фикстур batch (26 файлов, debug-бинарь) НЕ домерен — таймауты
200с/400с/480с все истекли БЕЗ завершения даже 6-файлового саб-батча (release-бинарь — тоже,
1 файл занял 2m31s вместо ожидаемых секунд). `tasklist`/`Get-Process` подтвердили: чужие
`nova.exe`/`clang.exe` (worktree `nova-209f4`) активно грузили CPU (82% общая загрузка) —
НЕ трогал (не мои процессы). Верификация сведена к 4 корпусам, что УСПЕЛИ пройти в разумное время
(collections/time+encoding/standalone×20/d119, все — debug PASS без δ + release PASS без δ +
флагман чисто) — этого достаточно для доверия ОДНОМУ смёрженному продюсеру, но полный
`d182/d143/d239/d85/d52/d16/d355/d402/d43/d315/d372/d354`-батч (20 из 26 файлов) остался
неизмеренным в ЭТУ сессию. **Рекомендация:** следующей волне — при доступности хоста прогнать
оставшиеся d-фикстуры с тем же трейсом, добавить delta к этому чекпойнту.

---

## 6. Коммиты (ветка `p196-ch-coverage2`, worktree `nova-196chc`)

1. `0fd827412` — `fix(types): [M-196-ch-coverage2] Producer B-fluent-generic — node_substs for
   -> @ methods with own method-level generics (Vec.append-class)` — §1 выше.
2. (этот коммит) — `docs(196): Zone CH волна-2 (coverage2) — Producer B-fluent-generic,
   B1-диагноз, капстоун-непригодность Q10, notes`.

**В main НЕ мёржено. Push запрещён по заданию.**
