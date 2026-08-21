# Memory Index

> Plan-status НЕ здесь: source of truth — docs/plans/README.md, simplifications.md, nova-private (см. feedback-no-external-memory-for-project-state). Этот индекс = только durable feedback/reference/operational. Одна короткая строка на запись; детали — в самом файле.

## Feedback — стиль работы
- [feedback-never-end-turn-on-report.md](feedback-never-end-turn-on-report.md) — не заканчивать ход докладом, пока работа не закрыта: последним обязан быть вызов инструмента
- [feedback-progress-not-activity.md](feedback-progress-not-activity.md) — активность ≠ прогресс: пробы и карты рисков не заменяют движение к цели
- [feedback-answer-short-and-simple.md](feedback-answer-short-and-simple.md) — на вопрос отвечать коротко и простыми словами, всегда
- [feedback-respond-in-russian.md](feedback-respond-in-russian.md) — отвечать по-русски, англицизмы к минимуму
- [feedback-solo-author-singular-voice.md](feedback-solo-author-singular-voice.md) — соло-проект: в публичных текстах «я», не «мы»
- [feedback-no-done-claims-on-documents.md](feedback-no-done-claims-on-documents.md) — не заявлять «готово» после документного ревью; гипотезы метить гипотезами
- [feedback-new-question-doesnt-cancel-earlier.md](feedback-new-question-doesnt-cancel-earlier.md) — новый вопрос не отменяет прежние; вести список незакрытых
- [feedback-dont-stop-to-ask-proceed.md](feedback-dont-stop-to-ask-proceed.md) — при «работай» не вставать на развилке с готовой рекомендацией
- [feedback-planning-task-not-execution.md](feedback-planning-task-not-execution.md) — «напиши план» = только текст; коммиты/гейты по явному слову
- [feedback-conventions-governance-integral.md](feedback-conventions-governance-integral.md) — конвенция даёт ответ → решай сам
- [feedback-verify-premise-before-work.md](feedback-verify-premise-before-work.md) — первым делом проверять премису запуском; закрывать по замеру, зелёный тест обязан уметь падать
- [feedback-zero-tolerance-bugs.md](feedback-zero-tolerance-bugs.md) — дефект чинится той же волной; обходы запрещены
- [feedback-never-accumulate.md](feedback-never-accumulate.md) — никогда не копи: незакрытая работа разбирается сразу
- [feedback-agent-token-economy.md](feedback-agent-token-economy.md) — лимиты: дешёвые модели, выдержки вместо доков, крупные батчи

## Feedback — эталоны и архитектура
- [feedback-early-go-as-mn-reference.md](feedback-early-go-as-mn-reference.md) — ранний Go на C = эталон M:N; паритет по скорости обязателен
- [feedback-rustc-as-reference.md](feedback-rustc-as-reference.md) — rustc = эталон типов/резолва/mono; AST-only без MIR = компромисс, не норма
- [feedback-compiler-fixes-checker-channel-196.md](feedback-compiler-fixes-checker-channel-196.md) — компилятор-фиксы ТОЛЬКО в чекер-канал; легаси emit_c не наращивать
- [feedback-maximize-nv-sourcing.md](feedback-maximize-nv-sourcing.md) — типы/функции из .nv по максимуму; в Rust только непортируемое
- [feedback-lang-change-needs-spec.md](feedback-lang-change-needs-spec.md) — язык-меняющее слияние не пушится без D-амендмента в том же слиянии

## Feedback — синтаксис и стиль Nova
- [feedback_nova_syntax.md](feedback_nova_syntax.md) — синтаксис не выдумывать: spec/decisions/ + examples/
- [feedback-sum-enum-marker-d406.md](feedback-sum-enum-marker-d406.md) — суммы только `type X enum A | B`; leading-| грепать
- [feedback-ctor-new-not-of.md](feedback-ctor-new-not-of.md) — конструкторы Type.new(...); .of только вариадик
- [feedback-vec-of-not-from-in-tests.md](feedback-vec-of-not-from-in-tests.md) — в тестах Vec[T].of(a,b,c)
- [feedback-with-star-and-property-methods.md](feedback-with-star-and-property-methods.md) — with_* всегда новое значение; поля = методы-свойства; цепочки по максимуму
- [feedback-nv-doc-comments-english.md](feedback-nv-doc-comments-english.md) — дока в .nv и линт-сообщения только по-английски

## Feedback — тесты и гейты
- [feedback-no-nova-codegen-direct.md](feedback-no-nova-codegen-direct.md) — `nova-codegen` напрямую не запускать, только через `nova.exe test`
- [feedback-test-conventions-strict.md](feedback-test-conventions-strict.md) — конвенции нормативны; раннер по маркеру EXPECT_*
- [feedback-module-tests-beside-module.md](feedback-module-tests-beside-module.md) — тесты std рядом с модулем; в nova_tests не писать
- [feedback-gate-filter-must-assert-pass-line.md](feedback-gate-filter-must-assert-pass-line.md) — гейт-фильтр обязан ассертить строку PASS/FAIL; не рапортовать счёт, которого не видел
- [feedback-isolate-conformance-before-push.md](feedback-isolate-conformance-before-push.md) — behavior-changing: прогнать conformance полным фильтром до пуша
- [feedback-nova-tests-not-correctness-gate.md](feedback-nova-tests-not-correctness-gate.md) — nova_tests не гейт корректности
- [feedback-codegen-dce-verification.md](feedback-codegen-dce-verification.md) — проверка codegen: baseline = kill-switch на том же бинаре
- [feedback-large-tests-stored-not-in-regress.md](feedback-large-tests-stored-not-in-regress.md) — большие тесты в репо, но не в дефолт-прогоне
- [feedback-lint-in-acceptance-ritual.md](feedback-lint-in-acceptance-ritual.md) — nova lint в приёмке каждой .nv-волны
- [feedback-spec-tests-batch-salvage.md](feedback-spec-tests-batch-salvage.md) — batch-workflow spec_tests: низкий yield, auto-salvage
- [feedback-no-interpreter.md](feedback-no-interpreter.md) — интерпретатор не делаем; только C-codegen

## Feedback — приёмы работы
- [feedback-no-backticks-through-shell.md](feedback-no-backticks-through-shell.md) — текст с обратными апострофами не писать через `python -c` в bash: оболочка съедает куски молча (наступил дважды 2026-08-08); скрипт файлом + самопроверка
- [feedback-reread-file-after-point-edits.md](feedback-reread-file-after-point-edits.md) — после серии правок перечитать файл целиком и грепнуть снятую форму на 0
- [feedback-read-files-efficiently.md](feedback-read-files-efficiently.md) — читать файлы целиком за раз
- [feedback-one-pass-fix.md](feedback-one-pass-fix.md) — Grep → Edit за один запуск
- [feedback-one-pass-debug-investigation.md](feedback-one-pass-debug-investigation.md) — debug не в два захода
- [feedback_nova_test_one_pass.md](feedback_nova_test_one_pass.md) — nova test один раз: summary + FAIL details
- [feedback_targeted_test_per_fix.md](feedback_targeted_test_per_fix.md) — per-fix только targeted фикстура
- [feedback-test-fix-per-file-loop.md](feedback-test-fix-per-file-loop.md) — массовые ошибки → per-file loop
- [feedback-commit-per-task.md](feedback-commit-per-task.md) — коммит по вехам, вехи разбиты по типам задач
- [feedback-update-logs.md](feedback-update-logs.md) — после задачи: project-creation.txt + discussion-log.md

## Feedback — агенты и модели
- [feedback-cheap-models-for-agents.md](feedback-cheap-models-for-agents.md) — haiku=механика (запрет main-репы, синтаксис по образцу), sonnet=карта, opus=разведка; модель указывать
- [feedback-haiku-not-for-classification.md](feedback-haiku-not-for-classification.md) — haiku не для классификации И не для длинных перечислений (зависал 2× по 600с)
- [feedback-opencode-for-agents.md](feedback-opencode-for-agents.md) — opencode работает; запись только внутрь проекта, cwd сбрасывает; бриф как для механизма
- [feedback-ultracode-only-196.md](feedback-ultracode-only-196.md) — Workflow/ultracode только для Plan 196
- [feedback_no_background_agents.md](feedback_no_background_agents.md) — спрашивать подтверждение перед фоновым Agent
- [feedback-subagent-must-not-wait-for-notifications.md](feedback-subagent-must-not-wait-for-notifications.md) — окно, ждущее нотификацию фона, мертво
- [feedback-agent-stall-1h-restart.md](feedback-agent-stall-1h-restart.md) — час без прогресса = стоп и перезапуск; 3-й провал → sonnet
- [feedback-workflow-agents-checkpoint-progress.md](feedback-workflow-agents-checkpoint-progress.md) — чекпоинт-файлы против rate-limit
- [feedback-agents-must-not-touch-git-config.md](feedback-agents-must-not-touch-git-config.md) — агентам запрет git config; чек %an перед push
- [feedback-parallel-session-compiler-queue.md](feedback-parallel-session-compiler-queue.md) — компилятор-брифы параллельной сессии исполнять самому
- [feedback-marker-numbers-tbd-not-self-assigned.md](feedback-marker-numbers-tbd-not-self-assigned.md) — номера маркеров не брать самому, писать №TBD
- [feedback-fable-delegate-max.md](feedback-fable-delegate-max.md) — себе только решения/слияния/гейты
- [feedback-plan172-whole-not-half.md](feedback-plan172-whole-not-half.md) — Plan 172 целиком, не половинить

## Feedback — git и worktree
- [feedback_git_add_specific.md](feedback_git_add_specific.md) — git add только по именам, никогда -A/.
- [feedback-conflict-marker-grep.md](feedback-conflict-marker-grep.md) — греп маркеров конфликта в одной команде с commit
- [feedback-verify-index-before-commit.md](feedback-verify-index-before-commit.md) — перед commit всегда git diff --cached --stat
- [feedback-commit-author-gitbash-unitcraft.md](feedback-commit-author-gitbash-unitcraft.md) — авторство и подпись = unitcraft из git bash
- [feedback-no-claude-coauthor.md](feedback-no-claude-coauthor.md) — не добавлять Co-Authored-By
- [feedback-push-after-green-gate.md](feedback-push-after-green-gate.md) — стоячее: пушить main сразу после зелёного гейта
- [feedback-ff-into-shared-main-repo.md](feedback-ff-into-shared-main-repo.md) — FF в main может попасть в чужую ветку; проверять HEAD
- [feedback-worktree-on-d-drive.md](feedback-worktree-on-d-drive.md) — worktree только в d:\Sources\nv-lang\
- [feedback-worktree-naming.md](feedback-worktree-naming.md) — постоянные worktree: nova-pNN
- [feedback-worktree-auto-register.md](feedback-worktree-auto-register.md) — регистрироваться самому первой командой
- [feedback-worktree-file-links.md](feedback-worktree-file-links.md) — ссылки в worktree абсолютным путём
- [feedback_worktree_cwd_clarity.md](feedback_worktree_cwd_clarity.md) — bash cwd дрейфует; git -C или абсолютный cd + verify
- [feedback-worktree-shared-stash.md](feedback-worktree-shared-stash.md) — worktree делят .git → не git stash
- [feedback-isolated-worktree.md](feedback-isolated-worktree.md) — изолированные задачи: свой постоянный worktree сразу
- [feedback-sequential-no-sub-worktree.md](feedback-sequential-no-sub-worktree.md) — многофазный план: без sub-worktree
- [feedback-sync-with-main-bidirectional.md](feedback-sync-with-main-bidirectional.md) — «синканись» = bidirectional, «обновись» = pull
- [feedback-sync-with-path.md](feedback-sync-with-path.md) — «синканись с майн PATH» = merge по пути назначения

## Feedback — доки и сайт
- [feedback-site-docs-guide-only.md](feedback-site-docs-guide-only.md) — сайт тянет только docs/guide/; docs/dev/ не линковать
- [feedback-no-external-memory-for-project-state.md](feedback-no-external-memory-for-project-state.md) — plan-status не из памяти
- [feedback-www-deploy-is-the-verdict.md](feedback-www-deploy-is-the-verdict.md) — www: «обновил сайт» = зелёный деплой Pages, не пуш; локальная sync+astro build до пуша
- [feedback-check-github-status-first.md](feedback-check-github-status-first.md) — отказ CI → сначала githubstatus.com

## User / Reference
- [user-identity.md](user_identity.md) — Головин Евгений (unitcraft), автор Nova
- [reference-msys2-grep-lc-all-c.md](reference-msys2-grep-lc-all-c.md) — стражи с не-ASCII обязаны export LC_ALL=C
- [reference-nova-module-model-folder.md](reference-nova-module-model-folder.md) — папка = один модуль из co-equal файлов
- [reference-nova-slice-vec-alias.md](reference-nova-slice-vec-alias.md) — []T = алиас Vec[T]
- [reference-nova-int-intptr-not-i64.md](reference-nova-int-intptr-not-i64.md) — int = intptr_t (как Go intgo), не int64_t
- [reference-mn-race-case-study.md](reference-mn-race-case-study.md) — case study STALE-slot race M:N; шаблон расследования
- [reference-gc-fresh-mono-safe.md](reference-gc-fresh-mono-safe.md) — fresh mono GC-безопасна; провалы = C-type clash
- [reference-claude-code-capabilities.md](reference-claude-code-capabilities.md) — шпаргалка по Artifact/Workflow/скиллам
- [reference-three-codegen-drivers.md](reference-three-codegen-drivers.md) — три драйвера кодогена; канал 196 — во все три + страж parity (№669, класс повторялся 3×)
- [reference-github-actions-autotrigger-silent-stop.md](reference-github-actions-autotrigger-silent-stop.md) — CI молча не стартует: признак = ноль check-suites

## Operational
- [project-nova-private-restored.md](project-nova-private-restored.md) — запись в журнал = немедленный пуш; пустая папка → переклонировать
- [project-package-repos-push-authorized.md](project-package-repos-push-authorized.md) — пакеты вливать и пушить самому; теги только по команде
- [project-three-remote-mirrors.md](project-three-remote-mirrors.md) — 9 реп на github+gitverse+sourcecraft; зеркала расходятся молча, сверять ls-remote
- [project-ci-monitoring-protocol.md](project-ci-monitoring-protocol.md) — CI github = авторитетный гейт; сторож только по команде
- [project-conformance-single-cu-run.md](project-conformance-single-cu-run.md) — conformance = один CU; команда запуска
- [project-nova-test-vs-test-build.md](project-nova-test-vs-test-build.md) — nova test = C-codegen pipeline
- [project-worktree-nova-test-setup.md](project-worktree-nova-test-setup.md) — env NOVA_GC_LIB_DIR/INCLUDE_DIR на main repo; libuv-submodule копировать
- [project-max-concurrent-windows.md](project-max-concurrent-windows.md) — не более 4 окон разом; тяжёлые команды интегратора добивают, окна валятся watchdog'ом
- [project-bash-timeout-10min-max.md](project-bash-timeout-10min-max.md) — таймаут потолок 600000мс; полный прогон дробить
- [feedback-background-shell-dies-use-foreground-timeout.md](feedback-background-shell-dies-use-foreground-timeout.md) — фон (run_in_background/nohup/start) молча умирает; долгое — foreground+timeout 600000, харнесс сам уведёт в фон; новый лог-файл на прогон
- [project-include-str-touch-trap.md](project-include-str-touch-trap.md) — touch точного пути external_registry.rs после правки .nv-снимков
- [project-z3-backend-ready.md](project-z3-backend-ready.md) — Z3 в vcpkg_installed; rebuild с --features z3-backend
- [project-plan70-model-settings.md](project-plan70-model-settings.md) — Model/effort/thinking per phase
- [project-spec-dblock-numbering.md](project-spec-dblock-numbering.md) — схема нумерации D-блоков
- [project-bug-number-sync-post-release-reminder.md](project-bug-number-sync-post-release-reminder.md) — после тега напомнить про судьбу правила №217
- [Бриф механическому исполнителю](feedback-mechanical-executor-brief.md) — для opencode обязательны 4 блока: команды целиком с путём к бинарю, точный путь в проекте, явный список запретов, ОБРАЗЕЦ строки результата; без них отчёт в произвольной форме
- [Не править напрямую в общем дереве](feedback-no-direct-edits-in-shared-main.md) — своя работа в своём worktree и слиянием; `git add -u`/`-A` запрещён наравне с `.`; столкновения были между двумя интеграторами в общем дереве, не между окнами
