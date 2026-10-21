# Задача 111 — дополнительный bounded круг 4

**Статус:** независимые extraction/guard исправления сохранены в `56a0a9e05`;
D420 не изменён, выбор manifest-provider seam согласуется с интегратором.
Это не повторная сдача и не объявление CI GREEN.

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
Это передано интегратору и на независимое мнение, выбор здесь не сделан.
В 221.1 заведена отдельная OPEN-запись `№TBD` о доказанном stale-success/no-lock;
её нельзя закрывать восстановлением compatibility parity. Интегратор
рекомендует provider seam владельцу, но owner GO пока ожидается.

Исходные watch-логи скопированы без изменения bytes в session pad:

- `round4-fresh-main-test.txt`, SHA256
  `1dc6903ddc222bb00f284e60cc6897c924c957c8f2c415e36395b5414d70f151`;
- `round4-linux-api-doc-tests.txt`, SHA256
  `2781b280bd29b8ad2b8785d2c751adbe4f6601d468320c17c2f5ff70c0d76095`.
