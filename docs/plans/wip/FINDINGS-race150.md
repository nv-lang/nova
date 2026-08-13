# RACE150 — Измерение гонки на общем mut-состоянии через границу файбера

## Вердикт

**Гонка ПРОЯВЛЯЕТСЯ.** Гипотеза №150 подтверждена: `nova check` молчит на обходной
паттерн (замыкание с mut-захватом, созданное снаружи `spawn`), а рантайм под нагрузкой
даёт неверный `len()` и/или SIGSEGV.

---

## (A) Контроль — прямой mut-захват в spawn

Файл: `race150/a_mut_capture_in_spawn.nv`

```nova
fn main() {
    mut v = Vec[int].new()
    supervised {
        spawn { v.push(1) }
    }
}
```

`nova check` даёт:

```
error: [E_CONCURRENT_MUT_CAPTURE] outer `mut` binding `v` captured by reference
in a `spawn`/`parallel for`/`detach` body: `Vec` is poisoned at `Vec.data`
(`* mut T`): a raw pointer (D415 poison base — no synchronization is expressible
through it; only the containing type's own `#share` vouch escapes) — under M:N
scheduling this alias is a data race (the child fiber may run concurrently with,
or migrate across threads from, the parent/siblings). Allowed captures
(Plan 173.3, D415 §2): move it in explicitly (`spawn consume v = expr { .. }` /
`spawn consume v { .. }` / `detach consume v { .. }`), capture it `ro`
(immutable view), or use an internally-synchronized `#share` type (`Mutex`/
`Atomic*` — or a user lock-free type vouched with `#share`).
```

**Компилятор ловит прямой захват. Гипотеза №150 — о том, что ОБХОД МОЛЧА.**

---

## (B) Обход границы — замыкание снаружи spawn

Файл: `race150/b_closure_bypass.nv`

```nova
fn parallel_spawn(f fn() -> (), n int, m int) {
    supervised {
        for _ in 0..n {
            spawn {
                for _ in 0..m {
                    f()
                }
            }
        }
    }
}

fn main() {
    mut v = Vec[int].new()
    ro push = || { v.push(1) }
    parallel_spawn(push, 8, 10000)
    ...
}
```

**`nova check`: ПРОХОДИТ** (0 errors, только предупреждения о неиспользованных
auto-import'ах из prelude).

### Таблица прогонов — N=8, M=10000 (ожидается 80000)

Две серии по 20 прогонов (40 суммарно):

| Серия | Прогоны | Неверный len | Краш (SIGSEGV) | Чисто |
|-------|---------|-------------|----------------|-------|
| 1     | 1–20    | 18          | 2              | 0     |
| 2     | 21–40   | 14          | 6              | 0     |
| **Итого** | **40** | **32**  | **8**          | **0** |

Наблюденные значения `got`: от **20828** до **55440** (ожидалось 80000).
Ни один прогон не дал правильного счёта.

### Таблица прогонов — N=16, M=100000 (ожидается 1600000, нагрузка)

| Прогон | Результат | got      |
|--------|-----------|----------|
| 1–3    | CRASH     | —        |
| 4      | RACE      | 271515   |
| 5      | RACE      | 282535   |
| 6–20   | CRASH     | —        |

**Итого (20 прогонов):** 2 неверный len, 18 краш, 0 чисто.

Под нагрузкой гонка деградирует в 90% крашей.

---

## (C) Контроль корректности — AtomicInt

Файл: `race150/c_atomic_control.nv`

```nova
fn main() {
    mut counter = AtomicInt.new(0)
    ro add = || { counter.fetch_add(1) }
    parallel_spawn(add, 8, 10000)
    ...
}
```

**20/20 чисто:** каждый прогон `expected 80000, got 80000`, 0 крашей. Доказывает,
что рантайм сам по себе стабилен — проблема именно в отсутствии синхронизации
общего Vec.

---

## Выводы

1. **Дыра подтверждена.** Компилятор проверяет захваты только на синтаксической
   границе `spawn { }`. Замыкание, созданное снаружи с mut-захватом `Vec`, проносит
   грязный указатель внутрь файберов без проверки.

2. **Гонка реальна и воспроизводима.** 40/40 прогонов B дали либо неверный счёт,
   либо краш. Ни один не был чист.

3. **Деструктивность растёт с нагрузкой.** При N=8 M=10000: 20% крашей. При
   N=16 M=100000: 90% крашей.

4. **AtomicInt — корректная альтернатива.** C проходит 20/20, доказывая, что
   проблема именно в синхронизации, а не в баге рантайма.

---

## Хеши коммитов

- `cbc2b022d` — repro + прогоны race150
