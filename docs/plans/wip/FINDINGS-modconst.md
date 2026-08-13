# MCFINDINGS — кросс-модульный module-const: проверка гипотезы

**Хеш коммита (базовый):** `286c9ca27893c4cf1cd80968214b432c1aa4857d`

**Модель:** Windows 10, clang, Dev mode.

**Компилятор:** `D:/Sources/nv-lang/nova/nova-cli/target/release/nova.exe` (готовый бинарь, не пересобирался).

**Окружение:**
```
NOVA_RT_DIR="$M/compiler-codegen/nova_rt"
NOVA_CG_INCLUDE="$M/compiler-codegen"
NOVA_GC_LIB_DIR="$M/compiler-codegen/vcpkg_installed/x64-windows-static/lib"
NOVA_INCLUDE_DIR="$M/compiler-codegen/vcpkg_installed/x64-windows-static/include"
NOVA_GC_INCLUDE_DIR="$NOVA_INCLUDE_DIR"
NOVA_STD_PATH — unset
```

---

## Шаг 1–2: `export const ANSWER int = 42` — кросс-модульный int const

Модуль `mcrepro/a/a.nv` объявляет `export const ANSWER int = 42`.
Потребитель `mcrepro/step2.nv` импортирует и использует `ANSWER` в test-блоке.

### Вывод
```
=== ШАГ 2: int const cross-module ===
Toolchain: clang, mode=Dev, jobs=16, paths=[D:\Sources\nv-lang\nova-modconst\mcrepro/step2.nv]
PASS           mcrepro/step2

===== SUMMARY =====
PASS: 1  FAIL: 0
```

**Результат: ЗЕЛЁНЫЙ.** Простой int const переходит границу модуля без проблем.

---

## Шаг 3: `export const ZERO MyVal = MyVal()` — const типизированного value-типа

Модуль `mcrepro/a/a.nv` содержит:
```nova
export type MyVal(hi i64 = 0, lo u64 = 0)
export const ZERO MyVal = MyVal()
```

Потребитель `mcrepro/step3.nv` импортирует `MyVal, ZERO` и обращается к полям.

### Вывод
```
=== ШАГ 3: typed const (value type) cross-module ===
Toolchain: clang, mode=Dev, jobs=16, paths=[D:\Sources\nv-lang\nova-modconst\mcrepro/step3.nv]
PASS           mcrepro/step3

===== SUMMARY =====
PASS: 1  FAIL: 0
```

**Результат: ЗЕЛЁНЫЙ.** Типизированный const value-типа переходит границу модуля без проблем — **при условии, что нет конфликта имён констант с другим модулем**.

---

## Шаг 4: импорт из `std.math.int128.{i128, ZERO, MIN}` с конфликтом имён

Потребитель `mcrepro/step4.nv` импортирует `ZERO` из **двух** модулей:
```nova
import mcrepro.a.{MyVal, ZERO, ONE}
import std.math.int128.{i128, ZERO, MIN}
```

### Вывод
```
=== ШАГ 4: импорт ZERO из std.math.int128 с конфликтом ===
Toolchain: clang, mode=Dev, jobs=16, paths=[D:\Sources\nv-lang\nova-modconst\mcrepro/step4.nv]
CC-FAIL        mcrepro/step4  # D:\Sources\nv-lang\nova-modconst\mcrepro/step4.c:1316:23: error: redefinition of '_nova_const_ZERO_value' with a different type: 'NovaTuple_i128' (aka 'struct NovaTuple_i128') vs 'NovaTuple_MyVal' (aka 'struct NovaTuple_MyVal') | D:\Sources\nv-lang\nova-modconst\mcrepro/step4.c:1317:23: error: redefinition of '_nova_const_ONE_value' with a different type: 'NovaTuple_i128' (aka 'struct NovaTuple_i128') vs 'NovaTuple_MyVal' (aka 'struct NovaTuple_MyVal') | D:\Sources\nv-lang\nova-modconst\mcrepro/step4.c:2582:16: error: returning 'NovaTuple_MyVal' (aka 'struct NovaTuple_MyVal') from a function w

===== SUMMARY =====
PASS: 0  FAIL: 1
```

**Результат: CC-FAIL (P67-LEGACY).** Ошибка codegen — C-компилятор видит переопределение символа `_nova_const_ZERO_value` с разными типами (`NovaTuple_i128` vs `NovaTuple_MyVal`).

### Контрольный прогон (шаг 4b)

Изолированный импорт только из `std.math.int128` (без `mcrepro.a`):
```nova
import std.math.int128.{i128, ZERO, MIN}
```

### Вывод
```
=== ШАГ 4b (контроль): изолированный импорт std.math.int128 ===
Toolchain: clang, mode=Dev, jobs=16, paths=[D:\Sources\nv-lang\nova-modconst\mcrepro/step4_isolated.nv]
PASS           mcrepro/step4_isolated

===== SUMMARY =====
PASS: 1  FAIL: 0
```

**Результат: ЗЕЛЁНЫЙ.** Изолированный импорт из `std.math.int128` **не вызывает** CC-FAIL.

---

## Вердикт

**Дефект ВОСПРОИЗВОДИТСЯ** — но с уточнением:

1. **Условие срабатывания:** checker пропускает импорт одноимённой константы (`ZERO`) из двух разных модулей, у которых тип константы различается. C codegen генерирует два определения C-символа `_nova_const_ZERO_value` с разными типами → C-компилятор отвечает `redefinition … with a different type`.

2. **Заявка «обращение к экспортированной константе ЧУЖОГО модуля роняет codegen»** — верна **только** при наличии в той же компиляции другого модуля, определяющего `export const` с тем же именем. Если потребитель импортирует `ZERO`/`MIN` из `std.math.int128` в изоляции — всё зелёное.

3. **Корень:** C codegen не манижирует имя модуля в символ C-переменной для констант. Используется голое имя константы (`ZERO` → `_nova_const_ZERO_value`), что неизбежно приводит к коллизии, когда разные модули определяют `export const` с одинаковым именем, но разными типами.

4. **Класс:** P67-LEGACY (codegen generates ill-formed C). Дефект подлежит регистрации в реестре.

### Файлы репро

```
mcrepro/
├── a/
│   └── a.nv              — модуль с export const ZERO MyVal (+ ANSWER, ONE)
├── step2.nv              — int const cross-module (PASS)
├── step3.nv              — typed const cross-module (PASS)
├── step4.nv              — импорт ZERO из ДВУХ модулей (CC-FAIL)
└── step4_isolated.nv     — импорт ZERO только из std.math.int128 (PASS)
```
