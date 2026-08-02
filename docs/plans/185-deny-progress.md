# Чекпоинт — Plan 185 Ф.3 хвост: `nova lint --deny` ([M-185-lint-deny-gate])

Статус: ✅ ЗАКРЫТО 2026-07-16. Ветка `p185-lint-deny`, worktree `nova-185deny`.

## Что сделано

- `nova-cli/src/main.rs`:
  - `Cmd::Lint` — новое поле `deny: Option<String>` (`--deny`, `num_args = 0..=1`,
    `require_equals = true`, `default_missing_value = ""`).
  - `cmd_lint(...)` — режим `DenyMode { Off, All, Rules(HashSet) }`; без `--deny`
    находки печатаются `warning:`, exit 0 (info-only); с `--deny` (bare) —
    ВСЕ находки `error:`, exit ≠0; с `--deny=W_X,W_Y` — только перечисленные id
    денай-ятся (валидация как у `--rule`), остальные остаются `warning:`-only.
  - `lints.rs` (реестр/`LintWarning`) НЕ тронут — severity/exit целиком в nova-cli
    (координация с 159.1-агентом: их зона — `collect_used_names`).
- `nova-cli/tests/lint_deny.rs` (новый) — 5 интеграционных тестов против
  собранного `nova`-бинаря (паттерн как `interp_unsupported.rs`):
  без-deny/exit-0, bare-deny/exit-1, selective-deny match/no-match, clean-file.
  Все 5 PASS.
- `docs/plans/185-nova-lint.md` — статус-блок + Ф.1/Ф.3/Приёмка обновлены.

## Явно ВНЕ периметра этой волны (по требованию координатора 2026-07-16)

- `.githooks/pre-commit` — НЕ трогать (правка была сделана и ОТКАЧЕНА
  `git checkout HEAD --`). Сейчас зовёт голый `nova lint` — под новой
  семантикой (exit 0 без `--deny`) хук больше не блокирует коммит на находках,
  если не добавить `--deny` явно. Решение — за владельцем.
- `.github/workflows/nova-lint.yml` — аналогично НЕ трогать (правка сделана и
  откачена). Жёсткие гейты (`nova-lint-std-gate`, `nova-lint-spec-tests-gate`,
  `nova-lint-registry-self-test`) сейчас зовут голый `nova lint` — та же
  семантическая дыра, решение за владельцем.
- `docs/dev/dev-workflow.md` — не редактировался (инструкция: вписывание
  `--deny` в приёмку — решение владельца).

## Побочная находка (не фикс, вне периметра)

`nova lint --deny std` НЕ чист: 3 находки `W_PARAM_NO_CONTRACT` в
`std/collections/{hashmap,queue,set}.nv` (`new(cap)` без `requires`) — дрейф
корпуса после lint-sanitation-волны 2026-07-10, не связан с `--deny`
(старый `nova lint std` тоже эту находку ловил и уже давал exit 1 ДО этого
фикса). Смежно, но не идентично `[M-lint-findings-param-no-contract]` в
backlog-followups.md (тот — про clamp-семантику Vec/SeekFrom, этот — про
голые `cap`-конструкторы). Не чинил — вне узкого периметра задачи.

## Также обнаружено (пре-существующий пробел, не мой)

`nova_tests/lint/conv_pos.nv` и `conv_clean.nv`, на которые ссылается CI job
`nova-lint-registry-self-test`, физически ОТСУТСТВУЮТ в репо (не создавал —
nova_tests устаревает, новое туда не пишут). CI-workflow эту правку не
получил (откачен), так что это существующее состояние main, не регрессия
от этой волны.
