# №1882 — абсолютный `NOVAC_SELF_PATH` на B1

## Наблюдение и границы доказательства

Сохранённые артефакты Windows x86_64 показывают path-sensitive отказ одного и того же `native-b1.exe`: при абсолютном `NOVAC_SELF_PATH` `emit novac/src/main.nv` завершился с rc=2 и `E_NOVAC_ICE`; при относительном `NOVAC_SELF_PATH=novac/src` сохранённый запуск завершился с rc=0 и выпустил C1 размером 13,250,302 байта, SHA-256 `25c5404f1e8e175652f19c9ca6cb03d29de82cee70a7cd9f4258c6724115c8a0`.

Это доказательство только наблюдавшейся path-sensitive разницы. Влияние на обязательный autonomous self-build gate не установлено: абсолютный путь дал ICE, относительный контроль прошёл, но обход не является исправлением. B1→B2/C1→C2 fixed point не проверен; задача #94 остаётся HOLD. Не делается вывод о внутреннем механизме embedded prelude.

## Зафиксированные входы и provenance

- B0 release asset: `nova-carina-0.2.0-windows-x86_64.zip`, SHA-256 `f9f973e2c83a930a68053b4952da47f4b72f8d50624fc6e86742da22fc4e12bf`.
- Распакованный `novac.exe`, SHA-256 `b3df460726e92eca612d7df3497c1a6184c07f415bf58f80001717cb2cf8536f`.
- `docs/dev/carina-seeds/windows-x86_64.json` указывает `source_revision` и `compiler_provenance` `99756a8867f913bd6156ed10accf8a20edea00d1`. Совпадение pin metadata и hash бинаря не доказывает, что именно эта ревизия baked внутрь бинаря; B0 в этой задаче не запускали.
- Сверка источника: bundled `std/src/os/pty.nv` имеет `join_nul(cmd.args)?` на строке 110 и `env_blob_of(deduped)?` на 113; текущий файл содержит те же выражения на 112/115. Это соответствует C0 trace 110/113 и C1 trace 112/115. Два comment hunks source diff каждый имеют net +1 перед соответствующим site. Это объясняет только номера строк trace metadata, не семантическое отличие C. Источник B1 — `57a468573018e0ea89c73d4eabd3330cbbd60d55`.
- `native-b1.exe` SHA-256 `1966922e47d7583838809ec13538d8ba75839440ff9e2f42d498fa8542df8add`.
- Абсолютный C output — диагностический JSON с `E_NOVAC_ICE` в `novac/src/pipeline/program.nv:157`: `pipeline: novac_fn_novac_diag_to_json__Diagnostic__nova_int__nova_str__to_nova_str belongs to module novac.diag, which the entry's imports do not reach`. Сохранённый `seed-b1-c1.stderr` пуст. Относительный C1 hash указан выше; его stderr также пуст.

## Команды A/B по сохранённым прогонам

Записанный `emit-c1.bat` работал из cwd `<B1_TREE>`, задавал абсолютный `NOVA_STD_PATH` на `<B1_TREE>\std\src`, затем абсолютный `NOVAC_SELF_PATH` на `<B1_TREE>\novac\src`. Нормализованная форма A (исполняемые файлы и outputs лежали в `%LOCALAPPDATA%\Temp\opencode\ses_mvanjiglayrltuom7q`):

```bat
cd /d <B1_TREE>
set NOVA_STD_PATH=<B1_TREE>\std\src
set NOVAC_SELF_PATH=<B1_TREE>\novac\src
%LOCALAPPDATA%\Temp\opencode\ses_mvanjiglayrltuom7q\native-b1.exe emit novac/src/main.nv > %LOCALAPPDATA%\Temp\opencode\ses_mvanjiglayrltuom7q\seed-b1-c1.c 2> %LOCALAPPDATA%\Temp\opencode\ses_mvanjiglayrltuom7q\seed-b1-c1.stderr
```

Ожидаемый сохранённый результат A: rc=2; `seed-b1-c1.c` содержит JSON `E_NOVAC_ICE`, `seed-b1-c1.stderr` пуст.

По сохранённым артефактам относительного контроля зафиксированы тот же B1, cwd, source и абсолютный `NOVA_STD_PATH`, а `NOVAC_SELF_PATH=novac/src`. Отдельный `.bat` этого запуска не сохранён; ниже — нормализованная реконструкция команды:

```bat
cd /d <B1_TREE>
set NOVA_STD_PATH=<B1_TREE>\std\src
set NOVAC_SELF_PATH=novac/src
%LOCALAPPDATA%\Temp\opencode\ses_mvanjiglayrltuom7q\native-b1.exe emit novac/src/main.nv > %LOCALAPPDATA%\Temp\opencode\ses_mvanjiglayrltuom7q\seed-b1-c1-relative.c 2> %LOCALAPPDATA%\Temp\opencode\ses_mvanjiglayrltuom7q\seed-b1-c1-relative.stderr
```

Для воспроизведения `<B1_TREE>` должен разрешаться в тот же сохранённый source/std snapshot, а не просто в текущий checkout.

Ожидаемый сохранённый результат B: rc=0; C size=13,250,302; SHA-256 `25c5404f1e8e175652f19c9ca6cb03d29de82cee70a7cd9f4258c6724115c8a0`; stderr пуст.

## Сборка B1: что известно и чего не хватает для точного повтора

Сохранённые `link-stage-b1.bat` и response files показывают сборку `native-b1.exe` из `seed-b0-emitted.c` (C0; 13,250,302 байта, SHA-256 `17e9ec674739011c9f30cad0289a4f8510087d915636404686f438e2074f2c52`) плюс C runtime sources из worktree задачи #94. Скрипт вызывает VS 2026 Build Tools Developer Command Prompt v18.0 для x64 и LLVM clang с target `x86_64-pc-windows-msvc`; сохранённые флаги включают `-O0 -g`, `-DNOVA_GC_BOEHM`, `-DGC_THREADS`, `-DNOVA_USE_LIBUV=1` и `-DNOVA_MAX_EFFECT_STORAGES=11`. `compile-stage-b1.bat` — syntax-only проверка C0, не полная сборка.

По отчёту #94 зафиксированы LLVM clang/clang-cl 22.1.5, Visual Studio Build Tools 18 (`vcvars` x64) и pinned submodule revisions из source tree 57a4685: `gc` `ee59af3722e56de8404de6cd0c21c2493cc4d855`, `libuv` `1cfa32ff59c076ffb6ed735bbc8c18361558661f`, `libatomic_ops` `4c00f978cbafe4eb76929dace4ba8f456e800fec`. Зафиксированы и SHA-256 библиотек: `libuv.lib` — `2c13429c4a238aa3299ca25028745d6ef171681bdd8c7e25ebbbd3b6fb5c4afe`, `gc.lib` — `72e3407a8385344afb2d263762cc256ed6b57b1f32649e7a804273b040b12a49`, `atomic_ops.lib` — `044400fe3b084f24ad0a98d6e20e3e888af8851c665da48c230c6de8cfe83a68`. Для точного побайтного повтора всё же недостаёт полного неизменяемого vcpkg binary-package manifest/snapshot, связанного с этим прогоном; кроме того, отдельный `.bat` относительного A/B прогона не сохранён (есть exit record, C1 и пустой stderr). Поэтому наличие записанных версий и библиотечных SHA не доказывает точную воспроизводимость B1 build; относительная команда выше нормализована по сохранённым результатам.

## Статус выполнения этой задачи

Этот документ составлен по сохранённым скриптам, C-файлам, stderr и exit records; описанные прогоны заново не запускались в задаче #95. Не запускать B0 или Oracle как judge/tool. Новые B2/C2 outputs и fixed-point proof здесь отсутствуют.
