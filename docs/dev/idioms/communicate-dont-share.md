# Идиома: communicate, don't share

> Plan 173.3 / [D415](../../../spec/decisions/06-concurrency.md#d415-data-race-freedom--share-атрибут-capture-check-consume-в-spawn-plan-1733).
> Гонка данных в Nova — **ошибка компиляции**, не рантайм-сюрприз.

## Правило одной фразой

Обычные данные шарятся даром (`ro` / авто-`#share`); отдать уникальное
мутабельное — `spawn consume`; шарить мутабельное — `Mutex`/`Atomic*`
(они сами `#share`); каналы — для передачи значений между файберами.
`#share` пишешь руками только если ты автор lock-free примитива.

## Что запрещено

```nova
mut acc = 0
supervised {
    spawn { acc = acc + 1 }   // ✗ E_CONCURRENT_MUT_CAPTURE — гонка под M:N
    spawn { acc = acc + 2 }
}
```

Внешний `mut`-биндинг не-`#share` типа, захваченный телом `spawn` /
`parallel for` / `detach` по ссылке, — это два файбера, пишущие в одну
ячейку без синхронизации. Компилятор отвергает на границе. (`detach`
добавлен амендментом D415 §2, 2026-07-11 — orphan-файбер, никогда не
join'ится с родителем, та же гонка.)

## Санкционированные пути

### 1. Канал — передать значение (предпочтительный путь)

```nova
ro (tx, rx) = Channel.new(1)
supervised {
    spawn { tx.try_send(compute()) }
}
tx.close()
ro result = rx.try_recv()          // Option[T]
```

`ro (tx, rx)` — захват `ro`, всегда легален (send/recv не требуют `mut`).

### 2. `Atomic*` — счётчик / флаг / скалярный результат

```nova
mut sum = AtomicInt.new(0)
mut done = AtomicBool.new(false)
supervised {
    spawn { sum.fetch_add(10) }
    spawn { sum.fetch_add(32); done.store(true) }
}
assert(sum.load() == 42)
```

### 3. `Mutex` — критическая секция над не-атомарным состоянием

```nova
mut mu = Mutex.new()
supervised {
    spawn {
        consume g = mu.lock()
        // критическая секция
        g.unlock()
    }
}
```

### 4. `spawn consume` — отдать уникальное во владение ребёнка

```nova
supervised {
    spawn consume file = File.open(path)! {
        // file принадлежит ребёнку; cleanup — на выходе ЕГО тела
        process(file)
    }
}
// `file` здесь не существует — use-after-consume невозможен by construction
```

### 5. `ro` — иммутабельный снимок для чтения

```nova
ro cfg = load_config()
supervised {
    spawn { handle(cfg) }      // deep-immutable view — легально
}
```

## Выбор между `consume` и `#share`

Разные паттерны, не альтернативы для одного значения: move = отдать
**уникальное** одному ребёнку; `#share` = **алиасить одно** среди многих
(Mutex/Atomic не clone-move'ятся). Тип может быть и `Clone`, и `#share` —
на use-site выбираешь одно.

## Почему не как в Go

Go ловит эту гонку только рантайм-детектором (`-race`, надо ВКЛЮЧИТЬ и
ПОПАСТЬ в интерливинг). Nova делает представление гонки невозможным на
уровне типов: «ядовитая база» (сырой `*T`, записываемая ячейка) транзитивно
снимает `#share`, эскейп — только аудируемый `#share`-vouch автора
синхронизированного типа (аналог Rust `unsafe impl Sync`).
