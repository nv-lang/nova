# Бриф интегратору — дефекты auto-deref вызова методов через указатель

Найдено док-сессией 2026-08-06 при перепроверке `docs/guide/typed-pointers.md`
под №353/№358 (пробы свежим `nova.exe` из main после слияния p-ptr-wave).
Номера не присваиваю — интегратору. Канал фикса — чекер
(`resolved_types`/`callees`, §0/196), НЕ легаси `emit_c.rs`.

## 1. `p.method()` через указатель тихо возвращает мусор (soundness)

Auto-deref вызов ОБЫЧНОГО метода через `*T` компилируется без единой
диагностики и возвращает неверное значение (похоже на биты адреса/ресивера):

```nova
module probes.pr11
type Acc6 { mut v int }
fn Acc6 @double() -> int => @v * 2
fn main() {
    mut a = Acc6 { v: 3 }
    ro q = (&a) as *Acc6
    println("direct=${a.double()} viaptr=${q.double()} field=${q.v}")
}
```

`nova build` + запуск (main, компилятор с волной p-ptr, 2026-08-06):

```
direct=6 viaptr=5455683428320 field=3
```

`nova check` — молчит; `nova test` вариант с `assert(q.double() == 6)` —
RUN-FAIL (т.е. это НЕ compile-гейт, а тихая неверная кодогенерация).
Чтение поля через тот же указатель (`q.v`) — корректно.

## 2. Вызов метода-свойства через указатель — ICE P67-LEGACY

Тот же класс, но для одноимённого метода-свойства поля (`@v()`):

```nova
type Acc3 { mut v int }
mut a = Acc3 { v: 1 }
ro q = (&a) as *Acc3
assert(q.v() == 1)     // ← ICE
```

```
nova: internal error at ...\emit_c.rs:59148: [P67-LEGACY] method call `.v`
return type unknown — checker must annotate (compiler-conventions.md §0);
obj_ty="const Nova_Acc3**"
```

Родственно ${a.v()}-крашу из BRIEF_ptr_l3_gap (тот же P67-LEGACY канал);
`obj_ty="const Nova_Acc3**"` — двойная звёздочка наводит на подозрение, что
ресивер передаётся без deref (возможно, общий корень с п.1).

## 3. Мелочь: подсказка `E_UNSAFE_REQUIRED` советует снятый `#unsafe`

Текст диагностики для `raw &x` вне unsafe заканчивается «…или mark enclosing
fn `#unsafe`» — атрибут `#unsafe` удалён (сам компилятор говорит об этом в
`E_UNSAFE_ATTR_DEPRECATED`, План 118.1.7). Должно быть «declare the fn
`unsafe fn`».

## Что уже сделано на стороне доки

Страница `docs/guide/typed-pointers.md`(+`.ru.md`) переписана под
методы-вместо-операторов (174.5) и №358; вызовы `p.method()` НЕ
документируются как рабочие — стоит честная оговорка «дефект передан в
работу, до фикса — `p.read().method()`». После фикса пп. 1–2 оговорку снять
(дока-сессия сделает по сигналу).
