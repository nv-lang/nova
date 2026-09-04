# Проба к №TBD — операторы над числовыми newtype стирают newtype

Заведено исследовательским окном (owner-research) 2026-09-04 по вопросу владельца «арифметика и
операторы сравнения в Нова работают над newtype от чисел?». Ответ: работают — и работают
**слишком широко**: два разных newtype над `int` сравниваются и складываются между собой и с
голым `int` без ошибки, тогда как тот же `TyId` в параметре типа `FnRow` отвергается `E7301`.
Спека этот случай не описывает: ни D52 (newtype), ни D46 (перегрузка операторов) не говорят,
что делает `==`/`<`/`+` с двумя разными newtype одного представления; `spec/conversions.ru.md:276`
говорит только, что `as` между ними — identity.

Исследование целиком: `docs/dev/research/2026-09-04-newtype-operators.md`; норма — амендмент к D52 от 2026-09-04.

## Как запускать

```sh
cp docs/plans/repro/904-newtype-operators-erase/<файл>.nv.txt docs/plans/repro/904-newtype-operators-erase/<файл>.nv
nova-cli/target/release/nova.exe check docs/plans/repro/904-newtype-operators-erase/<файл>.nv
nova-cli/target/release/nova.exe build docs/plans/repro/904-newtype-operators-erase/<файл>.nv -o <куда-нибудь>.exe
```

## Что замерено 2026-09-04 (owner-research, `nova-cli/target/release/nova.exe`)

| проба | что | `nova check` | `nova build` + stdout |
|---|---|---|---|
| `mixed.nv.txt` | `FnRow == TyId`, `FnRow < TyId`, `FnRow + TyId`, `FnRow == int`-переменная, `FnRow + int`-переменная | **`ok`** | собрано; печатает `true false 2 true 2` |
| `call.nv.txt` | КОНТРОЛЬ: `TyId` в параметр `FnRow` | `[E7301] cannot pass TyId as argument r of type FnRow` | — |
| `same.nv.txt` | КОНТРОЛЬ 2: операторы над ОДНИМ newtype, результат остаётся newtype | `ok` | собрано; печатает `true 3 2` |

Дефект — `mixed`. Класс тот же, что у №894 (`[]` не проверяет ключ, а `.index(k)` проверяет):
сахар оператора идёт другой дверью, чем вызов, и на этой двери различие newtype не смотрится.
`same` — то, что обязано сохраниться при любом фиксе: операторы над одним newtype законны, и
`a + b` даёт тот же newtype (нужен `as int`, чтобы напечатать).
