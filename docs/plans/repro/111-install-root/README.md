# Задача 111 — bounded rework, круг 2

**Статус:** локальные проверки выполнены; независимая приёмка и exact-SHA
CI/precheck остаются обязательными. Windows single-file/Clang, не MSVC;
№1889, №1890 и Ш.4 остаются OPEN. Production source не изменён.

## Четыре install-root marker: отказ и восстановление

Исполняемый candidate SHA256:
`c941afb4de0d1fe093766e73ddbfe475a13f4e7614cf3ad70bd2fd2422286c88`.
Новый воспроизводимый harness:
`scripts/guards/selftest/test-carina-install-root.py`. Он копирует staged
пакет в новый каталог **в переданном session scratch**, не изменяет исходный
пакет, перемещает ровно один marker и восстанавливает его в `finally`.
`cwd` всех запусков — внешний `external-project`, не checkout и не install.
Entry — копия существующей D89-фикстуры
`spec_tests/conformance/standalone/stdout_hello.nv`. Нельзя принять «отказ»
по недоступному seed/toolchain: проверяются **полный текст stderr**, пустой
stdout, exit 1 и отсутствие output EXE. До саботажей baseline собирается
и выполняется; после КАЖДОГО восстановления — такой же реальный build/run.

Команда (PowerShell, переменные — явные входы):

```powershell
python scripts/guards/selftest/test-carina-install-root.py --driver $driver --driver-sha256 $sha --install-root $stage --seed-zip $seed --scratch $sessionPad --clang $clang
```

В измеренном запуске `$stage` — сохранённый stage ранее проверенного ZIP
`389d58df85bb8f108d6b2560908cecb71e95cf2e748b6f2e1d5551fb6d3e6a24`;
`$sessionPad` — `D:\Temp\opencode\ses_mvf8jfko31nwxm58wp`;
`$clang` — `C:\Program Files\LLVM\bin\clang.exe`.
Pinned seed/source/asset не менялись. Полные build argv/stdout/stderr и
native stdout/stderr/exit сохранены в session pad:
`install-root-probes-ihq36but/receipts.json`, SHA256
`d579d593bad230fca0462b018ced6c87860f0f882e854d4c58b57bdb33c894b9`.

| Удалённый marker | Дословный итог отрицательной пробы (stderr ниже) | После восстановления |
|---|---|---|
| `docs/dev/carina-seeds/windows-x86_64.json` | `missing-1: RED exit=1 stdout=empty output_exists=false` | `restored-1: GREEN build_exit=0 native_exit=0 stdout=b'hello world from Nova\r\n' stderr=b''` |
| `scripts/tools/carina-gate.py` | `missing-2: RED exit=1 stdout=empty output_exists=false` | `restored-2: GREEN build_exit=0 native_exit=0 stdout=b'hello world from Nova\r\n' stderr=b''` |
| `novac/src` | `missing-3: RED exit=1 stdout=empty output_exists=false` | `restored-3: GREEN build_exit=0 native_exit=0 stdout=b'hello world from Nova\r\n' stderr=b''` |
| `nova_rt` | `missing-4: RED exit=1 stdout=empty output_exists=false` | `restored-4: GREEN build_exit=0 native_exit=0 stdout=b'hello world from Nova\r\n' stderr=b''` |

**Дословный stderr каждого из четырёх RED** (одна строка с завершающим LF):

```text
nova: entry is not associated with a Nova workspace or installation: D:\Temp\opencode\ses_mvf8jfko31nwxm58wp\install-root-probes-ihq36but\external-project\standalone\stdout_hello.nv
```

Финальная строка harness:

```text
test-carina-install-root PASS: four missing-marker REDs and four restored D89 GREENs
```

GREEN относится к реальному native результату, а не к graph guard или одному
обнаружению root. Соседняя полноценная установка физически присутствует
в session pad; ни один RED не подхватил её. Новые пробы не заменяют ранее
выполненную упаковку/распаковку ZIP и не доказывают Linux backend.

## Реестр package-release LSP target-dir mismatch

В `docs/plans/221.1-bug-sweep.md` добавлена запись `№TBD`, К2/P2:
класс — дублирование предположения о Cargo artifact path в consumer.
Оговорка явно оставляет overrides класса открытыми: исправление default
носителя и успешный package/install smoke не доказывают `CARGO_TARGET_DIR`
или `--target-dir`. Номер назначает интегратор, не исполнитель.

Проверка producer/consumer: из `nova-lsp/` выполнен
`cargo metadata --no-deps --format-version 1 --manifest-path Cargo.toml`.
Фактический `target_directory`:
`D:\Sources\nv-lang\worktrees\nova-opencode-111-otdelit-carina-build-drayver-ot-nova-cod\nova-lsp\../target`.
Это согласуется с `nova-lsp/.cargo/config.toml`, но не с прежним путём
`nova-lsp/target/release/nova-lsp.exe` в consumer.

До `367dd8528` полный package smoke упал ДО staging с
`nova-lsp.exe не найден` после успешной сборки. После исправления default
пути отдельный `-SkipBuild -SmokeTest` создал ZIP, распаковал его, выполнил
`nova --version`, собрал внешний hello-project через Carina и запустил
native EXE (`Hello, Nova!`, exit 0). Это измерение прошлого круга;
в круге 2 оно не заменяется «сборки трёх binaries успешны» и повторно
не выдаётся за новый package run.

Исходный полный RED-log скопирован без изменения bytes в session pad:
`package-smoke-original-red.txt`, SHA256
`7946e2ed409d8ea32a95d74091bd34e0e01f1338ae18048357a34add7bdff418`.
Registry single-verdict/routes и doc-conventions GREEN; entry-shape пока
RED исключительно на новой строке `№TBD`: до candidate-гейта интегратор
должен назначить номер. Этот RED не скрыт и baseline не ослаблялся.
