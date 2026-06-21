<!-- SPDX-License-Identifier: CC-BY-4.0 -->
# Как устроена разработка Nova и как подхватить работу

> **Кому:** новому агенту или человеку, который должен быстро понять, **что это за проект**
> и **как продолжать над ним работать**. Это не дубль [README.md](../README.md) (язык) и не
> [AGENTS.md](../AGENTS.md) (build/test для агентов) — это **связующий документ о процессе**:
> источники истины, план-ориентированная разработка, модель worktree, жёсткие правила, dev-логи.
> Подробности всегда в специализированных доках — здесь ссылки, не копии.

## TL;DR за 60 секунд

- **Nova** — системный язык с алгебраическими эффектами, структурной конкурентностью и опц. контрактами;
  компилятор на Rust, кодоген в C, рантайм на C (Boehm GC). См. [README.md](../README.md).
- **Три источника истины:** `spec/decisions/` (D-блоки = *почему*/семантика, нормативны) →
  `docs/plans/` (*что*/роадмап) → код (*как*/текущее поведение). **Spec-first:** семантику решаем в D-блоке
  **до** кода. При расхождении спека↔код — код = текущее поведение, спека = намерение; **не доверять одному
  слепо, проверять**.
- **Работа = планы.** Всё ведётся нумерованными самодостаточными планами в `docs/plans/`. Запуск: «**выполни
  план NNN**». Индекс — [docs/plans/README.md](plans/README.md).
- **Worktree на план.** Главный репо `d:/Sources/nv-lang/nova` — точка интеграции; каждый активный план
  живёт в соседнем worktree `../nova-pNN`. Сейчас их ~11.
- **Жёсткие правила:** никакого `git stash`; `git add` только по именам файлов; `git commit -s` (DCO),
  без `Co-Authored-By`; пересобрать `nova-cli` после правок `.rs`; тесты только через C-codegen.
- **После большой задачи:** обновить `docs/project-creation.txt`, `docs/simplifications.md` и discussion-log
  в **отдельном** репо `nova-private`.

---

## 1. Что это за проект

Nova — системный ЯП «для эпохи ИИ»: побочные эффекты видны в сигнатуре (`Db Net Fail`), что делает
ревью локальным, а тесты — без моков (подмена хендлера через `with`). Ставка: «LLM пишет, человек ревьюит».

Сейчас это **bootstrap-компилятор**, не дизайн-документ: парсер + чек + кодоген в C + нативный рантайм.
Один пайплайн (`nova build`/`nova test`) — **интерпретатора нет** (`nova run` не поддержан намеренно).

Глубже: [README.md](../README.md) (обзор + примеры), [spec/overview.md](../spec/overview.md),
[spec/effects.md](../spec/effects.md), [examples/getting_started.nv](../examples/getting_started.nv).

## 2. Карта репозитория (что важно для разработки)

| Путь | Что это |
|---|---|
| `compiler-codegen/` | Rust-крейт компилятора: `parser`, `types`, `codegen/emit_c.rs`, `test_runner.rs`, диагностика. Библиотека `nova_codegen` + бинарь `nova-codegen` (внутренний). |
| `compiler-codegen/nova_rt/` | C-рантайм: эффекты (`effects.h`), файберы, GC (`alloc_boehm.c`), libuv-планировщик. |
| `nova-cli/` | Пользовательский CLI `nova` (`build`/`run`/`test`/`check`/`doc`). **path-зависит** от `compiler-codegen` → правка `.rs` в кодогене авто-пересобирает `nova`. |
| `nova-lsp/` | LSP-сервер (`nova-lsp`). **Не трогать без явной задачи.** |
| `std/` | Стандартная библиотека на Nova (`prelude/`, коллекции, sync, net…). |
| `nova_tests/` | Тест-фикстуры `.nv` (folder-модули по темам/планам, `neg/` для compile-error). |
| `spec/` | Спецификация языка. `spec/decisions/` = **D-блоки** (нормативны). |
| `docs/` | Гайды разработчика + `plans/` + конвенции + dev-логи (`project-creation.txt`, `simplifications.md`). |
| `examples/` | Примеры на Nova. |
| `editors/` | Подсветка синтаксиса (VSCode/Vim/Emacs/Sublime). |

**Соседние рабочие копии (вне дерева репо):**
- `../nova-pNN` — worktree активных планов (одна `.git` с главным репо — см. §5).
- `../www` — **отдельный** репозиторий сайта nv-lang.org.
- `nova-private` — **отдельный** репозиторий: discussion-log, приватные заметки. Не в main-репо.

Сборочная модель: два независимых Cargo-крейта (`compiler-codegen`, `nova-cli` + `nova-lsp`) и
Nova-workspace `nova.toml` (members: `std`, `examples`, `nova_tests`). Подробнее: [docs/nova-cli.md](nova-cli.md).

## 3. Три источника истины (главная ментальная модель)

| Слой | Где | Отвечает на | Статус |
|---|---|---|---|
| **Решения (D-блоки)** | `spec/decisions/*.md` | *Почему* так устроено, какая семантика | **Нормативны.** Меняются записью нового/амендмента, не задним числом. |
| **Планы** | `docs/plans/*.md` (+ README) | *Что* делаем, в каком порядке, критерии приёмки | Рабочий роадмап, статусы CLOSED/IN-PROGRESS/retracted. |
| **Код** | `compiler-codegen/`, `std/` | *Как* сейчас реально себя ведёт компилятор | Истина о **текущем** поведении. |

Правила:
- **Spec-first:** изменение синтаксиса/семантики начинается с D-блока в `spec/decisions/`, потом код
  (`compiler-conventions.md` §5; пример — D315 «ResolvedType — единый носитель типа»).
- **Никогда не выдумывать синтаксис** по аналогии с другими языками — сверяться с `spec/decisions/` и
  `examples/`. Перед предложением идеи — проверить `spec/decisions/history/rejected.md` (может, уже отклонено).
- **Спека ↔ код расходятся** — это бывает (спека местами устаревает). Код = текущее поведение, спека =
  намерение. **Проверять оба, не верить одному слепо**; расхождение фиксировать (маркер/план), а не молча.

Структура D-блоков: 10 тематических файлов (`01-philosophy` … `10-overloading`) + `README.md` (индекс) +
`history/{rejected,evolution}.md`. Нумерация сквозная (D1…D315+); амендменты — `D216 V2/V3`; отозванные —
помечены `RETRACTED`. Свежий свободный номер уточнять по индексу (на момент написания: `D315` занят 172.1,
`D313` свободен).

## 4. Как организована работа: план-ориентированная разработка

- **План = самодостаточный файл** `docs/plans/NNN-<slug>.md` со всем контекстом для исполнения. Запуск
  одной фразой: «**выполни план NNN**». Внутри: фазы (`Ф.0`/`Ф.1`/…), критерии приёмки, источники, тесты.
- **Под-планы** наследуют номер: `169.1.2` — под-план `169.1`. Крупный план дробится на под-планы.
- **Индекс** всех планов и статусов — [docs/plans/README.md](plans/README.md) (большой файл, читать
  страницами через offset/limit).
- **Обязательный сквозной критерий** многих планов: «**без упрощений, как для прода**» — фаза не
  закрывается заглушкой/TODO; фазирование — это порядок, не урезание объёма.
- **Followup-маркеры `[M-…]`** — отложенная работа:
  - привязанные к плану → секция *Followups* того плана;
  - «плавающие» → [docs/backlog-followups.md](backlog-followups.md) (**только живые/открытые**) + запись
    в [docs/simplifications.md](simplifications.md) (**история**, append-only).
  - Жизненный цикл маркера описан в [AGENTS.md](../AGENTS.md#followup-markers-m).

## 5. Рабочий цикл (как подхватить и вести работу)

1. **Прочитать план** целиком (`docs/plans/NNN-*.md`) — он самодостаточен.
2. **Worktree на план.** Главный репо — точка интеграции; для плана — свой worktree:
   ```sh
   git worktree add -b plan-NN-<slug> ../nova-pNN main
   ```
   Соглашение имён: **`nova-pNN`** (не `nova-planNN`). На изолированной задаче создавать свой worktree
   сразу, а не переключать ветки в чужой рабочей копии. `git worktree list` — что сейчас занято.
3. **Менять код** spec-first (D-блок при изменении семантики).
4. **Собрать** (после любой правки `.rs`):
   ```sh
   cargo build --release --manifest-path nova-cli/Cargo.toml      # → nova-cli/target/release/nova(.exe)
   ```
   На worktree перед сборкой **обновить mtime** изменённого `.rs` (иначе cargo может не увидеть правку):
   PowerShell `(Get-Item path.rs).LastWriteTime = (Get-Date)`.
5. **Тестировать только через C-codegen** (не интерпретатор):
   ```sh
   nova-cli/target/release/nova test nova_tests/<папка>     # таргетно по ходу
   nova-cli/target/release/nova test --filter <substr>
   # одиночный дебаг с артефактами:
   compiler-codegen/target/debug/nova-codegen test-build FILE.nv --toolchain clang --keep-artifacts
   ```
   Per-fix — таргетная фикстура; полный `nova test` — в конце фазы. Полный регресс ~60-90 мин →
   **дробить на батчи < 10 мин** (потолок таймаута Bash/PS — 10 мин). Детали флагов и EXPECT-маркеров:
   [docs/test-conventions.md](test-conventions.md).
6. **Коммит по задаче** (одна задача → один коммит; несколько → несколько коммитов):
   ```sh
   git add <конкретные файлы>           # НИКОГДА -A / .
   git diff --cached --stat             # проверить индекс (чужие pre-staged правки!)
   git commit -s -m "fix(...): ..."     # -s = DCO Signed-off-by (CI требует); без Co-Authored-By
   ```
7. **Обновить dev-логи** (после большой задачи — см. §7).
8. **Синк в main** после фазы — двунаправленно: pull main → ветка (FF) **и** merge ветка → main (FF).
   FF в общий main может попасть в чужую checked-out ветку → проверять HEAD, двигать ref аккуратно
   (`git branch -f main` при необходимости).

## 6. Жёсткие операционные правила (нарушение = поломка у других агентов)

- 🚫 **Никакого `git stash`.** `.git` общая для всех worktree → stash/refs/reflog глобальны → коллизия и
  потеря чужих изменений. Для baseline-сравнения — **temp-worktree** (`git worktree add`) **или**
  **commit + reset** в своей ветке **или** **patch + checkout** (`git diff > p.patch; git checkout --
  <files>; …; git apply p.patch`). (`compiler-conventions.md` §7.5 синхронизирован с этим правилом.)
- 🎯 **`git add` только по именам файлов**, никогда `-A`/`.` — рядом работают другие агенты в параллельных
  worktree. Не включать в `git add` уже удалённые/перемещённые пути (иначе вся команда падает на bad
  pathspec и коммитятся только pre-staged чужие правки).
- 🔍 **Перед каждым commit — `git diff --cached --stat`**: в индексе могут лежать чужие pre-staged изменения.
- ✍️ **`git commit -s`** (DCO обязателен, CI гейтит). **Без `Co-Authored-By: Claude`** и подобных AI-trailer.
- 🔁 **Пересобрать `nova-cli` после любой правки `.rs`** (включая `compiler-codegen` — путь-зависимость).
  GC env (`NOVA_GC_LIB_DIR`/`NOVA_GC_INCLUDE_DIR`) для worktree-сборок — из главного репо.
- 🧪 **Тесты только C-codegen** (`nova test` / `test-build`); интерпретатор не используем.
- 📐 **Не выдумывать синтаксис** — `spec/decisions/` + `examples/`.
- 🤝 **Конвенции нормативны.** Любое изменение конвенции-дока **или** отклонение в коде — только по
  согласованию с владельцем + маркер `[M-*]` + запись в `backlog-followups.md`. Без самоправок и молчаливых
  отклонений (`docs/conventions-governance.md`).
- 🔕 **Фоновые агенты** (`run_in_background`/workflow) — спрашивать подтверждение перед запуском; ловят
  серверный rate-limit и падают → скрипты `.filter(Boolean)`, идемпотентность, чекпоинт-коммиты.

## 7. Куда писать после большой задачи

| Файл | Репо | Что |
|---|---|---|
| [docs/project-creation.txt](project-creation.txt) | main | Строка-итог по задаче (хронология создания проекта). |
| [docs/simplifications.md](simplifications.md) | main | История маркеров/упрощений (append-only). |
| `discussion-log.md` | **nova-private** (отдельный репо) | Развёрнутый лог обсуждений/решений. |

Стиль: тела коммитов и внутренние dev-логи — по-русски с английскими техническими терминами (house-style);
публичные доки (`README.md`, `AGENTS.md`, `CONTRIBUTING.md`) — по-английски.

## 8. Где сейчас идёт работа / как сориентироваться

- Текущие статусы и приоритеты — **только** в [docs/plans/README.md](plans/README.md),
  [docs/simplifications.md](simplifications.md), `nova-private/`. Внешним заметкам про статус не доверять.
- Активные направления видно по worktree (`git worktree list`) и по индексу планов. На момент написания
  крупные узлы: унификация системы ошибок/cleanup (план 173 + 174/175/176), единый type-engine 172.x
  (D315 ResolvedType), консолидация тестов 169.1.x.
- Открытые «плавающие» долги — [docs/backlog-followups.md](backlog-followups.md).

## 9. Частые грабли

| Грабля | Как правильно |
|---|---|
| Правишь `.rs` в worktree, cargo «не видит» изменений | Обнови mtime файла перед `cargo build` (PowerShell `LastWriteTime`). |
| `git add` падает на bad pathspec | Не включай удалённые/перемещённые пути; добавляй только существующие. |
| Тест классифицируется «не туда» | Классификация — по **маркеру** `EXPECT_*` (первые ~30 строк), не по папке/суффиксу. |
| Net/concurrency-тесты падают вне главного репо | libuv/GC берут `repo_root = current_dir`; гонять из соответствующего worktree, env из main. |
| Спека противоречит коду | Код = текущее поведение; спека = намерение. Проверить оба, расхождение зафиксировать. |
| Полный `nova test` убивается по таймауту | Дробить на батчи < 10 мин (потолок таймаута инструмента). |

## 10. Карта документов (куда смотреть за деталями)

| Тема | Документ |
|---|---|
| Онбординг агента (build/test/правила) | [AGENTS.md](../AGENTS.md) |
| Обзор языка + примеры | [README.md](../README.md), [spec/overview.md](../spec/overview.md) |
| Вклад, DCO, лицензии | [CONTRIBUTING.md](../CONTRIBUTING.md) |
| Решения/семантика | [spec/decisions/README.md](../spec/decisions/README.md) |
| Все планы + статусы | [docs/plans/README.md](plans/README.md) |
| Тесты (маркеры, folder-модули, флаги) | [docs/test-conventions.md](test-conventions.md) |
| Правила разработки компилятора | [docs/compiler-conventions.md](compiler-conventions.md) |
| Управление конвенциями (мета) | [docs/conventions-governance.md](conventions-governance.md) |
| CLI-справка | [docs/nova-cli.md](nova-cli.md) |
| Открытые долги `[M-*]` | [docs/backlog-followups.md](backlog-followups.md) |
| Модель ошибок/cleanup | [docs/idiom/error-and-cleanup-model.md](idiom/error-and-cleanup-model.md) |
