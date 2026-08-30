<!-- SPDX-License-Identifier: CC-BY-4.0 -->
КЛЕТКА | cli | К3

# Охота: ОРАКУЛ × CLI и первая диагностика (2026-08-30)

**Первая охота трека `oracle`** — план 278 Ф.7, пункт релиза 221 A-V17. Модель
агента: **opus** (defect-hunter). Клетка назначена окном: то, что видит внешний
человек в первые пять минут. Эталон — спека и `docs/guide/nova-cli.md`; на этом
треке архитектурной сетки нет (границa названа в `LEDGER.md`).

Бинарь: `nova-cli/target/release/nova.exe`. Пробы — `probes/2026-08-30-cli-first-contact/<имя>/`,
команда воспроизведения у каждой находки своя (`probe.sh` там, где нужен
дифференциал).

## Находки

НАХОДКА | oracle | cli | f06-prelude-absent-silent | `env -u NOVA_STD_PATH nova check hello.nv` → rc=1 «undefined identifier `println`» на ДОСЛОВНОМ hello-world из `docs/guide/quickstart.md:61-77` | отсутствует ПРЕЛЮД, а компилятор винит верный исходник; резолвер импортов на тот же дефицит отвечает громко (`cannot find module … searched:` с тремя путями) — `compiler-codegen/src/imports.rs:267` молчит, соседняя дверь говорит
НАХОДКА | oracle | cli | f05-doc-wrong-extension | `nova doc hello.txt` → rc=0, печатает `#  ()` и выходит УСПЕХОМ; `nova check hello.txt` → rc=2 «not a Nova source» | `nova-cli.md:214-215` требует rc=2 на чужом расширении; `cmd_doc` (`main.rs:3013-3040`) — единственный из семи входов без проверки, шесть остальных несут ту же строку. Класс «тихий успех» (№770)
НАХОДКА | oracle | cli | f02-exitcode-no-nova-toml | `nova check` / `nova test .` / `nova regen-runtime --check` без манифеста → rc=1 | `nova-cli.md:161,185-188` обещает rc=2 («missing nova.toml»); носитель — `main.rs:1221` возвращает `anyhow!` вместо `usage_err`
НАХОДКА | oracle | cli | f08-exitcode-file-not-found | несуществующий файл: check/build/test/consume-analyze/gc-layout-analyze → rc=2, а doc/test-build/contracts/bench/doc-query → rc=1 | `nova-cli.md:161` требует rc=2 для «file not found»; носители `main.rs:3032`, `:5928`. Побочно: три из шести печатают сырую ошибку ОС в СИСТЕМНОЙ ЛОКАЛИ
НАХОДКА | oracle | cli | f10-format-short-not-greppable | `nova check --format short bad.nv` → каждая строка получает префикс `<file>: `, и `grep -cE '^[^ ]+\.nv:[0-9]+:[0-9]+: '` даёт **0** | `nova-cli.md:224` обещает «`short` (`file:line:col: msg` for grep)» с образцом `:244-248`; ни одна строка образцу не соответствует
НАХОДКА | oracle | cli | f11-test-nopath-order | `nova test` в ЧУЖОМ проекте → rc=1 и попытка `git submodule update --init` в каталоге пользователя, «FATAL libuv submodule not initialized»; та же команда в монорепе → rc=2 «requires at least one path» | `main.rs:5717-5726`: `detect_toolchain`/`detect_or_build_libuv` стоят ПЕРЕД проверкой пустого списка путей, и ошибка употребления превращается в совет чинить сабмодуль компилятора
НАХОДКА | oracle | cli | f04-bom-not-stripped | `nova check bom.nv` → rc=1 «unexpected byte: 'ï'», сниппет при этом рисует строку валидной | D104 нормативен (`spec/decisions/03-syntax.md:6112`): «BOM в начале файла снимается перед doc-recognition»; лексер не снимает (`lexer/mod.rs:316` печатает сырой байт 0xEF как `'ï'`, хотя символ — U+FEFF)
НАХОДКА | oracle | cli | f13-uppercase-ext | `nova check HELLO.NV` → rc=2 «not a Nova source», `nova check hello.nv` на ТОМ ЖЕ файле той же регистронезависимой ФС → rc=0 | у `HELLO.NV` расширение `.nv` и есть; семь мест сравнивают `== Some("nv")` регистрочувствительно
НАХОДКА | oracle | cli | f01-help-russian | `nova --help` → 12 строк кириллицы («lint  Plan 185: машинные проверки конвенций», «--verify  Включить SMT-верификацию…») | `AGENTS.md` §Language: диагностические тексты — по-английски; носители `nova-cli/src/main.rs:75, 212, 259, 290, 300`
НАХОДКА | oracle | cli | f03-russian-diagnostics | русские тексты ошибок `nova add`/`update`/`bench` и ПОЛНОСТЬЮ русский УСПЕШНЫЙ вывод `nova info` («Пакет: … Effect-surface: ∅») | то же правило; носители `main.rs:4116, 4199, 4304, 4446`. `nova-cli.md:357-359` называет `info` «Nova-unique» — то есть витриной
НАХОДКА | oracle | cli | f12-json-schema-russian | `nova doc --json-schema` → 11 русских строк внутри схемы под собственным `"$id": "https://nova-lang.org/schemas/nova-doc-v1.json"` | не диагностика, а ОПУБЛИКОВАННЫЙ машиночитаемый артефакт с внешним идентификатором
НАХОДКА | oracle | cli | f07-testbuild-dir-lies | `nova test-build sub` (каталог) → rc=1 «file not found: sub», хотя каталог существует; `nova build sub` → rc=2 «requires a single .nv file, got directory» | сообщение утверждает ложное о ФС; два места отвечают на один вопрос по-разному (`main.rs:5928` против ветки `build`)
НАХОДКА | oracle | cli | f14-info-points-elsewhere | `nova info nosuch.nv` → rc=2 «nova.toml без секции [package]», тогда как манифест на месте; `nova check nosuch.nv` → «path not found: nosuch.nv» | диагностика указывает не на ту причину (`main.rs:4313`)
НАХОДКА | oracle | cli | f09-unterminated-block-comment | `nova check t.nv` → «expected fn / type / let / const / test» | список рекламирует `let`, снятый D184, — и тот же компилятор на `let` печатает `[E_KW_REMOVED_LET]`. Два адреса, один вопрос, разные ответы: `parser/mod.rs:2154` против `:6123`. Список не называет ни `import`, ни `module`, ни `export`, ни `effect`
НАХОДКА | oracle | cli | f15-diagnostic-dumps-whole-long-line | длинная строка → диагностика 21157 байт: сниппет 20041 символ, каретки 550 | окна/усечения нет; эталоны (rustc/clang) длинную строку сужают. Проверка глубины при этом работает: на 512 приходит `[E_NESTING_TOO_DEEP]`, паники нет

## Что обошёл и почему

- **Полный `nova build` вне монорепы** (Nova → C → exe): требует MSVC/clang и
  пяти переменных окружения, прогон — минуты. Не обстреляны `-o` в
  несуществующий каталог, в существующий каталог, без прав. Назван как самый
  вероятный оставшийся носитель класса «тихий успех».
- **`setup-env.ps1`**: в дереве его нет, он рождается `scripts/package-release.ps1`.
  Значит путь внешнего человека (распаковал zip → dot-source → `nova --version`)
  проверен только со второй ступени; первая не пройдена ни разу.
- **`doc-mcp`, `daemon`, `lint`, `doc --watch`, `bench remote`, `add/update` по
  git-зависимости** — первые три отсутствуют в справочнике вовсе (см. Противоречия).
- **Не-Windows**: всё мерено на Windows 11, локаль ru-RU. На другой локали часть
  находок про сырые ошибки ОС выглядит иначе, но коды возврата останутся.
- **Паник не нашёл ни одной.** Обстреляно: несуществующий файл, каталог вместо
  файла, файл без расширения, пустой, из пробелов, 4 КБ случайных байт,
  невалидный UTF-8, BOM, верхний регистр расширения, пути с пробелами и
  кириллицей, вложенность 500/2000/10000, битый JSON в `doc-query` и `bench
  diff`, неизвестный флаг, флаг без значения, `--jobs abc`, `--jobs -1`,
  `--timeout 0`, `--timeout -5`, `--toolchain nosuch`, `--format bogus`.
  Это утверждение о ПОИСКЕ, а не о предмете.

## Противоречия

**(а) Обязателен ли путь у `nova test` — внутри ОДНОГО файла доки.**
`docs/guide/nova-cli.md:450` — `nova test [PATH]... [--filter …]` (скобки =
необязательно); `docs/guide/nova-cli.md:462` — `| PATH... | — (required) | …
(at least one required) |`. Выбор не охотника.

**(б) Законен ли `let` — внутри ОДНОГО файла компилятора.**
`parser/mod.rs:2154` — `"expected fn / type / let / const / test, got {}"`;
`parser/mod.rs:6123` — `"[E_KW_REMOVED_LET] `let` keyword removed in Plan 114 (D184)"`.

**(в) Какие команды существуют — дока против бинаря.**
`nova-cli.md:22-36` перечисляет 15 команд; `nova --help` — 19: сверх списка
`lint`, `gc-effect-analyze`, `gc-layout-analyze`, `daemon`. Четыре команды видны
пользователю в первом же `--help` и не описаны нигде.
