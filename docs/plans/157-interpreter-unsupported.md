<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# Plan 157: Tree-walking interpreter — UNSUPPORTED (C-codegen only)

> **Создан:** 2026-06-14.
> **Статус:** ✅ DONE (2026-06-14) — `nova run` громко ошибается, мёртвые interp-тесты
>   удалены, user-facing доки + сайт почищены, `nova-cli` доки сверены с реальным CLI.
> **Spec:** [D274](../../spec/decisions/08-runtime.md#d274). **Open:** [Q-interpreter-future](../../spec/open-questions.md).
> **Ветка:** `chore-disable-interp-nova-run` (worktree `nova-noninterp`); сайт — `chore-remove-nova-run` (repo www).
> **Маркер:** `[M-interp-unsupported]`.

---

## 1. Зачем
Древесный интерпретатор (`nova run`, модуль `compiler-codegen/src/interp/`) расходился
с C-семантикой и тормозил разработку. Единственный поддерживаемый и тестируемый путь —
**компиляция в C** (`nova build` / `nova test` / `nova test-build`). Этот план делает это
явным в коде и доках. «пока» — формулировка намеренная (возможна полная вырезка ЛИБО
восстановление — [Q-interpreter-future](../../spec/open-questions.md)).

## 2. Что сделано
1. **`nova run` застаблен.** Команда остаётся **видимой** в CLI (discoverability), но при
   вызове немедленно завершается с ошибкой и подсказкой `nova build <file>` / `nova test`
   (exit ≠ 0). help помечен `[UNSUPPORTED]`. Сознательная **громкая** граница, не тихий no-op.
2. **Код-пометка.** `interp/mod.rs` — `//!`-нота «НЕ ПОДДЕРЖИВАЕТСЯ, kept for reference».
3. **Мёртвые interp-тесты удалены** (ссылались на изъятый библиотечный крейт `nova`):
   `compiler-codegen/tests/integration.rs`, `spec_nova.rs`, `common/mod.rs`,
   `nova-cli/tests/run_interp_named.rs`.
4. **Доки `nova-cli.md`/`.ru.md`** сверены с реальным CLI (`nova --help`): добавлены
   недостающие `consume-analyze`, `bench field-cache`, семейство `--field-cache-*`;
   `nova run` помечен unsupported.
5. **User-facing доки + примеры** (README[.ru], examples/*) — `nova run FILE` → `nova build
   FILE -o bin && ./bin`. **Сайт www** (6 партиалов) — то же.
6. **Тест-контракт:** `nova-cli/tests/interp_unsupported.rs` — negative (`nova run` ошибается
   + указывает на C-codegen) + positive (`nova check` работает); прогон через **релизный** бинарник.

**Вне scope (намеренно):** историю (планы/spec-архивы/логи) не переписывал; внутренний
dev-инструмент `nova-codegen run`/`test-interp` НЕ застаблен (см. [Q-interpreter-future]).

## 3. Критерии приёмки
1. ✅ `nova run FILE` → exit ≠ 0 + сообщение «interpreter … not supported» + указание на
   `nova build`/`nova test`. (Проверено релизным бинарником + регресс-тест.)
2. ✅ `nova --help` помечает `run` как `[UNSUPPORTED]`.
3. ✅ Поддерживаемый путь не сломан: `nova check` тривиальной программы → exit 0 (positive-тест).
4. ✅ Мёртвые interp-тесты удалены; их класс ошибок (`use nova::` → изъятый крейт) исчез;
   прочие интеграционные таргеты (`git_dep_e2e`/`version_resolve_e2e`/`lockfile_repro`) собираются.
5. ✅ В user-facing доках и на сайте нет инструкций использовать `nova run` (только описания
   «не поддерживается»).
6. ✅ **«Без упрощений как для прода» — обязательный критерий:** громкая ошибка (не silent
   no-op), полноценный негативный+позитивный тест через релизный бинарник, конкретные
   удаления вместо «закомментировать», доки приведены в соответствие.

## 4. Тесты
- `nova-cli/tests/interp_unsupported.rs` — `nova_run_is_unsupported` (negative) +
  `nova_check_still_works` (positive). Прогон: `cd nova-cli && cargo test --release --test
  interp_unsupported` → 2 passed / 0 failed.
- Изоляция фикстуры: каждый тест пишет `.nv` в **свою** temp-поддиректорию (Nova трактует
  папку как один folder-module из co-equal файлов — общий temp дал бы duplicate `main`).

## 5. Остаток
[Q-interpreter-future](../../spec/open-questions.md): полная вырезка vs сохранение `interp/`;
застабить ли внутренний `nova-codegen run`/`test-interp`; нестыковка `docs/nova-codegen.md`
(сайт уже помечен unsupported). Маркер `[M-interp-unsupported]` в backlog.

### 5.1. Residual закрыт (2026-06-14) — Q-interpreter-future ✅ RESOLVED
Внутренний dev-инструмент и доки приведены к тому же «unsupported»-контракту, что и
user-facing `nova run`:
- **`nova-codegen run` / `test-interp` застаблены** (`0d7116f4`): handlers больше не
  конструируют `interp::Interpreter`, а громко ошибаются (exit ≠ 0) с указанием на C-codegen;
  clap doc-строки помечены `[UNSUPPORTED]`. `compile`/`check`/`test-build`/прочее работают.
- **`docs/nova-codegen.md`/`.ru.md`** выверены (тот же `0d7116f4`): `run`/`test-interp`
  помечены `[UNSUPPORTED]`, `interp/` описан как «kept for reference, не подключён».
- **Регресс-тест** `compiler-codegen/tests/interp_tool_unsupported.rs` (`a4e26525`): negative
  (`run` + `test-interp` ошибаются) + positive (`compile` работает) — 3/3 PASS через релизный
  бинарник. `cargo build --release` зелёный.
- **`interp/` оставлен «для справки»** (НЕ удалён) — consistent с «пока» / D274. Полное
  удаление модуля сознательно **ОТЛОЖЕНО** (единственный residual) — `[M-interp-unsupported]`.

Ветка `chore-disable-interp-codegen-tool` (worktree `nova-interp2`). Plan 157 остаётся
✅ DONE; Q-interpreter-future — ✅ RESOLVED.
