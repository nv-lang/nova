# Задача 111 — дополнительный bounded круг 4

**Статус:** независимые extraction/guard исправления сохранены в `56a0a9e05`;
owner GO «Сохранить compat» передан интегратором. Provider seam реализован,
D420 не изменён. Это локальные targeted proofs, не объявление full CI GREEN.

## Выполненные независимые исправления

- Unix unit test `test_runner::tests::archive_key_sees_the_compiler` сохранён
  со всеми ассерциями и обращается к извлечённым support API. Linux WSL:
  `1 passed; 0 failed; 1242 filtered out`. Support doctests: 0 failures;
  codegen doctests: 2 ignored, 0 failures (не называются двумя выполненными
  положительными тестами).
- Support unit tests: 69 passed, 0 failed. Новые property tests проверяют
  все пары git pin selectors, маршрутизацию override assignments и точные
  десять CP1251 codepoints. Саботаж `pin_count > 1` → `> 2` краснит тест
  `every_pair_of_git_pins_is_rejected` на паре rev/tag; восстановление даёт
  2/2 manifest property tests и 1/1 CP1251 test GREEN.
- Guard выдаёт машинную строку `check-nova-driver-no-codegen ok:`;
  побайтовая проверка подтвердила LF-only stdout/stderr. Его назначение
  добавлено в `docs/dev/rules-for-agents.md`. Production Cargo crates
  `nova-driver` и `nova-driver-support` внесены в root allowlist с причиной.
- Десять CP1251 символов — намеренные данные таблицы, не порча текста.
  Записаны точными Unicode escapes без изменения runtime mapping. Прямой
  no-mojibake scan: 139 signature lines, 418 replacement lines; baseline
  усилен 144 → 139, allowance старого `test_runner.rs` удалён (там теперь 0).
- Пять invariant findings раскрыты конкретными свойствами: enum pin +
  rejection count, единственная insertion-ветка override map, создание
  `.git` до sync с проверкой результата, ассерты разрешённого legacy proxy.

На checkpoint после коммита:

```text
check-invariant-discipline ok: новых инвариантов на честном слове нет
check-no-mojibake ok: новой порчи текста нет (139 <= 139, замен 418 <= 418); носителей в базе 39, осмотрено файлов 9570 >= 7642
check-guard-honesty ok: стражей проверено 431 (.sh) и 209 (.py), проб-улик 332 (.sh), CRLF 0, слепых к имени бинаря 0, съедаемых оболочкой сообщений 0
check-gate-steps-assert ok: каждый шаг гейта требует от стража его собственную строку
check-repo-root-clean ok: в корне только предусмотренное (файлов 17, каталогов 28)
```

## D420: причинное base-vs-candidate сравнение без новой политики

Fresh main после fetch: `7ed5c7c2643642bf782be781cb2217759a000198`.
Baseline snapshot собран в session pad из `git archive` этого SHA, без
импорта ветки 110. Тот же integration test
`plan204_local_toml_and_replace_gate::add_external_path_with_flag_succeeds`:
base GREEN (1/1), candidate RED (`E_DEP_PATH_OUTSIDE_REPO`).

Исполняемые файлы в причинном runtime repro:

- baseline SHA256
  `3047286352abcb18804c6ca081e7af7d93c2c3f520df5b9606852f1858feb231`;
- candidate checkpoint `56a0a9e05`, SHA256
  `6680adbe001040dd98831ea7257f5942f0b5d66581bff33b342aae682abd13e9`.

Без флага обе программы отказывают (exit 2), manifest не меняется и lock
не возникает. С явным флагом base сообщает `locked: nova.lock.toml updated`,
exit 0, но lock файла НЕТ. Candidate изменяет manifest, затем отказывает,
exit 1, lock также не возникает. Receipt с полными bytes:
session pad `round4-cli-runtime-en573r_i/receipts.json`, SHA256
`786181ef872ed2558855d03befa95a9662572e867874e17c33509ac711fc94e7`.

Для причинной проверки использован существующий рубильник `NOVA_SRC_INDEX=0`:
никакие исходники или binaries между этими запусками не менялись. Перед
каждым запуском один и тот же manifest восстанавливается побайтно; cwd,
цель зависимости и остальные входы совпадают. Меняются только executable
и значение рубильника; обе версии запускаются отдельными процессами.

| Источник path | Binary | Snapshot default | `NOVA_SRC_INDEX=0` |
|---|---|---|---|
| Внешний + explicit flag | base | exit 0, lock отсутствует | exit 1, lock отсутствует |
| Внешний + explicit flag | candidate | exit 1, lock отсутствует | exit 1, lock отсутствует |
| Внутрирепный, без флага | base | exit 0, lock отсутствует | exit 0, lock создан |
| Внутрирепный, без флага | candidate | exit 0, lock создан | exit 0, lock создан |

На внешнем path stderr baseline с выключенным snapshot **дословно совпал**
со stderr candidate: `E_DEP_PATH_OUTSIDE_REPO` на одинаковом manifest path.
Полные receipts: session pad
`round4-snapshot-causal-1c2q7i5d/receipts.json`, SHA256
`524c2eb907c994d010b77a67f798ce163371a95efcc6198d6655efdb4b54e850`.

**Локализованный класс регресса extraction:** base lock APIs читают compiler
`manifest::parse_manifest`, связанный с source-index snapshot; перенесённый
lock читает support parser напрямую из FS. `cmd_add` сначала парсит старый
manifest, затем пишет новый и вызывает sync. Base sync повторно получает
старый snapshot, candidate — новое содержимое. Одинаковый enforcement
судит разные входы; отключение snapshot на том же base binary сводит
поведение к candidate. Внутрирепный контроль показывает, что это не только
policy external-path: base также теряет новый lock при разрешённом path.

Новый bypass/флаг для обычной резолюции не вводился. Сохранение compatibility
manifest-view через provider seam и исправление самого stale-snapshot
дефекта — разные решения; первое сохраняет существующий base observable
defect, второе затрагивает документированное расхождение D420/конвенций.
После причинного A/B owner GO «Сохранить compat» передан интегратором:
разрешён только provider seam, без one-shot exception или D420 policy fix.
В 221.1 отдельная OPEN-запись о stale-success/no-lock получила назначенный
интегратором №1893; её нельзя закрывать восстановлением compatibility parity.

Исходные watch-логи скопированы без изменения bytes в session pad:

- `round4-fresh-main-test.txt`, SHA256
  `1dc6903ddc222bb00f284e60cc6897c924c957c8f2c415e36395b5414d70f151`;
- `round4-linux-api-doc-tests.txt`, SHA256
  `2781b280bd29b8ad2b8785d2c751adbe4f6601d468320c17c2f5ff70c0d76095`.

## Provider seam после owner GO

- `ManifestProvider` — явный function pointer, без global/thread-local
  production override, без доступа support к compiler crate. Default support
  API передаёт fresh FS parser. Compatibility wrappers передают существующий
  `compiler-codegen::manifest::parse_manifest` со snapshot-view.
- Reader проходит через graph recursion, root enforcement, replace warnings,
  sync/load_pins/ensure_pins_loaded/update/update_precise и Git version provider.
  Условия D420 не менялись. Production driver defaults не переключались.
- Два новых support tests проверяют read-view, реальные lock bytes и запрет
  внешнего path для обоих readers в sync/load_pins. Саботаж: заменить reader
  graph на fresh parser внутри `sync_ex`; новый read-view test RED с exit 101
  на `sync_with_provider(...).is_empty()`. Восстановление: support 71/71 GREEN.
- Неизменённые CLI add tests: 3 passed, 0 failed, 1 filtered out. Это не
  объявление четвёртого build-теста успешным — он в этот targeted run не входил.

`manifest-provider-parity.py` воспроизводит 12 пар запусков base/candidate:
external с флагом, external без флага, internal; snapshot default/disabled;
изначально отсутствующий/существующий lock. Сравнение включает exit, stdout,
stderr, **точные bytes manifest и lock**, lock existence. Все 12 пар совпали.
При default snapshot и новом manifest с добавленной dependency обе версии
сохраняют известный stale-success/no-lock; при internal + disabled snapshot
обе реально создают lock с `extlib`. При external + disabled snapshot обе
дают прежний `E_DEP_PATH_OUTSIDE_REPO`, существующий lock не меняется. Без
флага обе отказывают exit 2 без изменения manifest и lock.

Первый receipt: session pad `manifest-provider-parity-_8p6smcr/receipts.json`, SHA256
`667a63f944089b6a67ba89301548c905a37f38ac262af349934868f7fd32b843`.
Дополнительно sabotage compatibility wrapper → fresh `support::sync` краснит
неизменённый `add_external_path_with_flag_succeeds` (exit 101,
`E_DEP_PATH_OUTSIDE_REPO`); после восстановления snapshot wrapper add 3/3 GREEN.
Повторный receipt после восстановления:
`manifest-provider-parity-a8wzew05/receipts.json`, SHA256
`002f1085d0da4419fe3b2e6bcba07da23a4a3a8596c6ea69bb3f31a78e540cf4`;
candidate compatibility executable SHA256
`e7b5ba239f571950880783bae7e2dc7a9bd2950560d516170dceec4ee826c5e5`.
После финальной перестановки документации Rust (без изменения логики)
обе программы вновь пересобраны и матрица повторена: финальный receipt
`manifest-provider-parity-5oesu6zc/receipts.json`, SHA256
`6a999a46c6dc8f72bff80b12ab9db50f9de5b901950477dc6615f419a4c91e18`.
Финальные executable SHA256: compat
`2273275dad6badad619d8b1331c18c3bf770ca834687897fd4ba3ddfdcf75054`,
production debug driver
`bf755c44a1cb6fddb27a77b105ff6c20b9ec48bdabdb868773dd3b5a4e959890`.
Полный JSON хранит bytes в hex и SHA256 каждого binary. Скрипт находится рядом
с этим отчётом; параметр `--work` обязан указывать session scratchpad.

Реестр пересчитан непосредственно: rows 1801, max 1893, dupes 0; относительно
`53ffb2a2d` прежние numbered rows не удалены, добавлена только №1893. В baseline
обновлены rows/max и пояснение; gaps не менялись. №1892 и №1893 остаются OPEN.

Предыдущие install-root/B1/generated-C/native golden receipts — исторические
доказательства соответствующих checkpoint, а не повторные прогоны нового SHA.
№1889/№1890 и Ш.4 остаются OPEN; задача 110 не импортировалась.

## Команды текущего targeted checkpoint

Все команды ниже запущены из дерева задачи; `TEMP`/`TMP` направлены в session
pad. Для parity probe `--baseline` — сохранённый baseline binary, `--candidate`
— пересобранный `nova-cli/target/debug/nova.exe`, `--work` — session pad.

| Команда | Exit | Дословный результат |
|---|---:|---|
| `cargo test --locked --manifest-path nova-driver-support/Cargo.toml` | 0 | `test result: ok. 71 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 5.27s` |
| `cargo test --locked --manifest-path nova-cli/Cargo.toml --test plan204_local_toml_and_replace_gate add_ -- --nocapture` | 0 | `test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 1 filtered out; finished in 0.82s` |
| `cargo build --locked --manifest-path nova-driver/Cargo.toml` | 0 | ``Finished `dev` profile [unoptimized + debuginfo] target(s) in 1.80s`` |
| `python scripts/guards/check-nova-driver-no-codegen.py . --self-test` | 0 | `check-nova-driver-no-codegen ok: production graph clean` |
| `bash scripts/guards/check-invariant-discipline.sh .` | 0 | `check-invariant-discipline ok: новых инвариантов на честном слове нет` |
| `bash scripts/guards/check-repo-root-clean.sh .` | 0 | `check-repo-root-clean ok: в корне только предусмотренное (файлов 17, каталогов 28)` |
| `bash scripts/guards/check-no-mojibake.sh .` | 0 | `check-no-mojibake ok: новой порчи текста нет (139 <= 139, замен 418 <= 418); носителей в базе 39, осмотрено файлов 9571 >= 7642` |
| `python scripts/guards/check-guard-honesty.py .` | 0 | `check-guard-honesty ok: стражей проверено 431 (.sh) и 209 (.py), проб-улик 332 (.sh), CRLF 0, слепых к имени бинаря 0, съедаемых оболочкой сообщений 0` |
| `bash scripts/guards/check-gate-steps-assert.sh .` | 0 | `check-gate-steps-assert ok: каждый шаг гейта требует от стража его собственную строку` |
| `bash scripts/guards/check-rules-page-complete.sh .` | 0 | `check-rules-page-complete ok: новых неназванных нет; старый долг 40 ровно по базе` |
| `bash scripts/guards/check-registry-entry-shape.sh .` | 0 | `check-registry-entry-shape ok: роста неоформленных записей нет (343 <= 343, номера сверены поимённо), разобрано строк 1801 >= 1142` |
| `python scripts/guards/check-registry-rows-intact.py .` | 0 | ``check-registry-rows-intact ok: записей 1801 (база 1801), наибольший номер 1893 (база 1893), дублей 0, законных дыр 92, из них протухших (номер и в `gaps=`, и со строкой) 0 (база 0) — ни одна строка не потеряна`` |
| `bash scripts/guards/check-registry-routes.sh .` | 0 | `check-registry-routes ok: открытых блокеров тега 109 (база 109), K1 без маршрута 0 (база 0), без оговорки 0 (база 0)` |
| `bash scripts/guards/check-registry-single-verdict.sh .` | 0 | `check-registry-single-verdict ok: у каждой строки реестра ровно один действующий вердикт, статус и маршрут; вердиктов без статуса 19 при базе 19 (номера сверены поимённо), разобрано строк 1801 >= 1142` |
| `python scripts/guards/check-registry-status-field.py .` | 0 | `check-registry-status-field ok: строк без поля 659 (база 659), новых нет` |
| `python scripts/guards/check-registry-past-tense-status.py .` | 0 | `check-registry-past-tense-status ok: закрытий прозой без поля 29 (== база); новых не прибавилось` |

Первоначальные ошибочные вызовы отсутствующих `.sh` для Python honesty/status
guards дали exit 127; это ошибки вызова, не результаты guard. Выше приведены
исправленные реальные Python-запуски. RED sabotage-прогоны перечислены отдельно
и после них изменения восстановлены. Старые tests/fixtures не менялись.

Финальная runtime-команда (PowerShell; `$env:TEMP` — session pad):

```powershell
python docs/plans/repro/111-install-root/manifest-provider-parity.py --baseline "$env:TEMP\round4-main-baseline\nova-cli\target\debug\nova.exe" --candidate nova-cli/target/debug/nova.exe --work $env:TEMP
```

Exit 0, последняя строка дословно:
`receipt_sha256: 6a999a46c6dc8f72bff80b12ab9db50f9de5b901950477dc6615f419a4c91e18`.
