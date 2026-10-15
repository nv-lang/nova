# Замер KNOWN-RED №685: `call_operands/pos_1`

**Статус:** №685 остаётся открытым известным дефектом Oracle 0.1.x. Эта запись KNOWN-RED — узкое исключение только из платформенного differential Windows для этой фикстуры; она не объявляет поведение нормой языка, не закрывает Carina/D65 и не утверждает, что дефект исправлен. После исправления №685 запись `scripts/guards/novac-known-red-685.json` удаляется, и `pos_1.nv` снова судится обычным differential.

## Источник и точность замера

Фикстура [`novac/fixtures/call_operands/pos_1.nv`](../../../novac/fixtures/call_operands/pos_1.nv), SHA-256 `cf891011fa4ab84bdfd7216c0db276dd8ab32d529b722134004e7a0e5611b8ec` (955 байт), не менялась. В замер вошли вызовы в коротком замыкании и операторах: строки 18, 20, 22 и 24 (`mark("x", 0) > 1 && mark("y", 1) > 0`, сумма `mark("p", 1) + mark("q", 2)`, `||` и `mark("m", 3) * mark("n", 4) - mark("o", 5)`). Замер сделан из исходников HEAD `e9bcd5e0f352429a4b99c4801e96ae145617f694`:

| Роль | Выбранный бинарь | SHA-256 |
|---|---|---|
| Windows Oracle CLI | `nova-cli/target/release/nova.exe` | `e53494bd864ede43b7c6ba62a01eb2f2d97828208a6df9e83af54b3686f6dd0f` |
| Windows Carina CLI | `novac/target/novac.exe` | `ff198acb608bab9d50d6cbaf62150e0623e5d5dbd1eb784d3a8cfa6c9a2b3b7b` |
| Oracle program, созданный smoke из этой фикстуры | `oracle-cf891011fa4ab84b-pos_1-t1792096240.exe` в scratch-кэше сессии | `1ac4e0c84b851509ce63aeb06916291c2be0ed981c59739988b84350680f4207` |

Путь Oracle зафиксирован `novac-e1-smoke.sh` в `oracle.path`: `nova-cli/target/release/nova.exe`; Carina задан явно через `NOVAC_BIN=novac/target/novac.exe`. Прогон smoke — watch `1792096798861-mkg430`; он завершился кодом 1 именно из-за stdout divergence и показал обе стороны diff. Sidecar с отдельными stdout/stderr/exit: `pos1-signature3.json` в scratch `nova-89-report-known-red-oracle-685`.

Нормализация платформы — только `CRLF → LF`; остальные байты не меняются. На Windows обе программы завершились с exit 0, stderr пустой. Нормализованные stdout:

**Oracle:**

```text
eval x
false
eval q
eval p
3
eval u
true
eval o
eval n
eval m
7
```

**Carina:**

```text
eval x
false
eval p
eval q
3
eval u
true
eval m
eval n
eval o
7
```

Подпись точного нормализованного наблюдения (она же вывод строки KNOWN-RED):

| Сторона | Exit | SHA-256 stdout | SHA-256 stderr |
|---|---:|---|---|
| Oracle | 0 | `050b06ea48f5754186e4ff7ff3d4ea86c48bfbb26986ab6cad1b9028e4281bc3` | `e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855` |
| Carina | 0 | `83d9a5f51f93170f457f0b2f503bd03cc2083460dfd1615302635dfe49297606` | `e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855` |

## Платформенная граница

Измеренная сигнатура закреплена **только для Windows**. Linux не получает фиктивной записи или исключения: там `pos_1` остаётся обычным differential, и любой mismatch — RED. На Linux 1-я сборка Oracle с rustc 1.93.1 (`01f6ddf7588f42ae2d7eb0a2f21d44e8e96674cf`, host `x86_64-unknown-linux-gnu`) упала ICE в `check_mod_deathness` при выдаче существующих `dead_code`-warnings; разрешённый повтор с единственной диагностической опцией `RUSTFLAGS=-Adead_code` завершился успешно и выдал `novac/target/linux-oracle/release/nova` (SHA-256 `1d081e568665e1a11cb34aa964320ba61e9ea0fcf562ccc84bf577ba2ebf8195`). Опция меняла только предупреждения хост-компилятора, не исходники продукта. Из этого Oracle собрана локальная **неопубликованная Carina B-кандидатная сборка** командой `novac/target/linux-oracle/release/nova build novac/src/main.nv -o novac/target/linux-carina/novac`: исходник `novac/src/main.nv` SHA-256 `3104bbe6b6deea8995d49dc78a7f36fe9096bb6437eeb937b41426d247e19da8`, бинарь `novac/target/linux-carina/novac` SHA-256 `1dc5f2699de1c322675c309146593a0541c64c7858835698a6918cadd943261c`. Оба Linux `check` этой фикстуры завершились exit 0. Runtime smoke (watch `1792098599999-5944mk`) остановился до запуска программ: установленный clang 21.1.8 ответил `invalid linker name in argument '-fuse-ld=lld'` — в этой WSL-среде нет lld. Поэтому Linux runtime stdout/stderr/exit не измерялись и Linux-сигнатура в KNOWN-RED не записывается; обычный Linux differential и отсутствие ложного KNOWN-RED должны быть доказаны exact candidate CI.
