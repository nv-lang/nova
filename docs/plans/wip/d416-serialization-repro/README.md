# Репро №173: D416§2 сериализация Supervisor.on_child_fail НАРУШЕНА

Фикстура задумывалась ПИНОМ гарантии (окно 238/A-V10) и в прогонах окон
проходила. Приёмка A-V10 (2026-07-31): в мега-CU RUN-FAIL, изолированно
5 падений из 10 — неатомарный счётчик в on_child_fail теряет обновления.
Гарантия «сериализовано на drive-файбере, по одному, в порядке слотов»
(D416§2) рантаймом НЕ выполняется.

Следствия (все исполнены той же волной): carve-out Supervisor в энфорсе D441
ОТОЗВАН; сайты polaris с mut-захватом в on_child_fail мигрированы на Atomic;
D416§2 в спеке помечен нарушенным. Файл здесь — репро для M:N-окна
(№173, вместе с №165/№169); когда рантайм починит сериализацию — фикстура
возвращается в conformance как пин, carve-out можно вернуть.

**Оригинальная (`supervisor_on_child_fail_serialized_pin_test.nv`) БОЛЬШЕ НЕ
КОМПИЛИРУЕТСЯ** — carve-out отозван, mut-счётчик в `on_child_fail` теперь
ловится `E_HANDLER_MUT_CAPTURE_IN_FIBER`. Используйте
`probe173_atomic_checker_clean.nv` (AtomicInt-only, D441-совместимая) —
измеряет `race_detected` (реентрантность хендлера) и `total_calls` vs
`n*rounds` (событие-потеря, №243).

## Окно p-vela (2026-08-02) — новая находка, ОПРОВЕРГНУТА ещё одна гипотеза, root cause НЕ закрыт

**Базовая статистика** (`probe173_atomic_checker_clean.nv`, n=64, rounds=20,
worktree `nova-vela`, 30 изолированных прогонов собранного `.exe`):
**PASS: 20/30** (~33% потерь). `race_detected == 0` во ВСЕХ прогонах —
опровержение прошлого окна (неатомарный счётчик из-за конкурентного вызова
хендлера) переподтверждено.

**Метод (state-dump / point-probe по debugging-races.md §1):**
1. Добавлен C-уровневый счётчик `children_started` (инкремент в САМОМ начале
   тела каждого spawn, до `throw`) — во ВСЕХ лоссовых прогонах
   `children_started == n*rounds` РОВНО. Значит каждый ребёнок реально
   стартовал; потеря НЕ в диспетчеризации spawn.
2. Инструментирован writer (`nova_fiber_report_child_kinded`, per-slot
   publish) и reader (`nova_supervised_process_decisions`, dispatch-loop) —
   `writes` (writer-счётчик) ВСЕГДА равен `n*rounds` (1280) НА КОНЕЦ
   ПРОЦЕССА; `dispatched` (reader-счётчик) короче ровно на число потерянных
   раундов. `writes_default_path == 0` (не default-path), `skipped_cancel_kind
   == 0` (НЕ CANCEL-kind миссклассификация), `reentrant_skip == 0` (гейт
   `_deciding` не срабатывает).
3. Slot-collision скан (лог `(q,slot,msg)` каждой publish-записи, разбит на
   блоки по 64 = один раунд): **ЗА ДЕСЯТКИ ЛОССОВЫХ прогонов — ноль
   коллизий.** Значит `nova_scope_alloc_child_slot` НЕ дублирует индекс
   слота между детьми одного раунда (опровергает гипотезу гонки на
   allocator'е).
4. **Ключевая находка:** прямо в точке, где owner-поток впервые наблюдает
   `pending_remote == 0` (брейк из drain-цикла, `nova_supervised_run_impl`),
   скан `child_error[].published` (acquire-load) по всем `child_count`
   слотам показывает **63 из 64** в лоссовом раунде — притом что
   `_diag_remote_inc == _diag_remote_dec == 64` (все 64 инкремента/декремента
   `pending_remote` РЕАЛЬНО произошли, подтверждено отдельным shared
   struct-полем на `NovaFiberQueue`, НЕ per-TU static — типовая ловушка:
   `static`-глобали в `fibers.h` независимы по TU, per-TU diag-счётчики врали
   бы при сравнении writer(generated .c)/increment(runtime.c)).
5. **Решающий эксперимент (опровергает ordering/visibility как причину):**
   на «пропавшем» слоте — `nova_thread_fence_seq_cst()` (максимально
   сильный барьер) + `uv_sleep(5)` (5 МИЛЛИСЕКУНД — гигантский запас против
   любой реальной задержки cache-coherency, которая на x86 — наносекунды) +
   повторный `nova_thread_fence_seq_cst()` + повторный load — **published
   ОСТАЁТСЯ false**. Это ИСКЛЮЧАЕТ гипотезу «видимость, просто ещё не
   долетело» — если бы это была чистая memory-ordering задержка, 5ms
   гарантированно хватило бы. Запись физически ЕЩЁ НЕ ПРОИЗОШЛА на момент
   скана, хотя decrement, который «должен» идти СТРОГО ПОСЛЕ неё в program
   order той же нити (см. codegen `emit_spawn`: report_child_kinded →
   free_slot → `pending_sweeps++` → `pending_remote--`), уже засчитан.

**Опробованный (и ОПРОВЕРГНУТЫЙ статистикой) фикс:** `pending_remote`/
`pending_sweeps` decrements переведены с `nova_aint_fetch_sub_release`
(RELEASE-only) на новый `nova_aint_fetch_sub_acqrel` (ACQ_REL) — теоретически
обоснованно (release-only decrement, читаемый ПОСТОРОННИМ потоком through
plain acquire-load, — задокументированная дыра: release-sequence гарантирует
happens-before только с ГОЛОВОЙ цепочки, не транзитивно со ВСЕМИ N
контрибьюторами, если только не КАЖДЫЙ decrement acq_rel). **После правки —
статистика НЕ изменилась (19/30 PASS, тот же порядок, CI перекрываются)** —
подтверждено п.5 выше: раз даже SEQ_CST-барьер+5ms-сон не помогает, дело
вообще не в ordering. Правка ОТКАЧЕНА (не даёт объективного улучшения, не
годится как «фикс», codebase возвращён в pristine — см. `git log` окна
p-vela, коммитов в main НЕ было).

**Рабочая гипотеза для следующего окна (НЕ доказана):** раз запись физически
не произошла, а counter уже досчитал до нуля — вероятен ДВОЙНОЙ/ФАНТОМНЫЙ
decrement, не соответствующий РЕАЛЬНОМУ ребёнку ТЕКУЩЕГО раунда. `q`
(`NovaFiberQueue`) в пробе — стековая переменная, ФИЗИЧЕСКИ переиспользуемая
на том же адресе КАЖДЫЙ раунд (подтверждено логами: один и тот же указатель
`q=...` во всех 20 REMOTE-ZERO печатях одного прогона). Гипотеза: медленный
«отставший» ребёнок ИЗ ПРЕДЫДУЩЕГО раунда (чей decrement почему-то не был
дождан раундом-владельцем — то есть САМ факт «раунд закончился с 63/64»
может быть СЛЕДСТВИЕМ этой же проблемы в ПРЕДЫДУЩЕМ раунде, самоподдерживающийся
паттерн) применяет свой decrement/write К СЛЕДУЮЩЕМУ раунду, потому что
`_nova_parent_scope`-указатель, захваченный при spawn, — это просто СЫРОЙ
адрес, которому всё равно, какое «поколение» `q` сейчас там лежит. ПЕРВОЕ
(изначальное) появление 63/64 у round-1-подобного изолированного случая
этой гипотезой НЕ объясняется напрямую — нужен par independent root cause
ИЛИ доказательство, что фантомный decrement возможен БЕЗ предыдущего раунда
(напр. двойной sweep одного ребёнка, `nova_scope_retain_or_release_child`/
`nova_scope_sweep_dead_child` — НЕ проверено этим окном).

**Не проверено / следующие шаги:**
- Трассировка КОНКРЕТНОГО «пропавшего» child (slot, `_nova_parent_slot`) —
  доходит ли ЕГО СОБСТВЕННЫЙ decrement до ЭТОГО `q`, или decrement,
  засчитанный за него, принадлежит ДРУГОМУ ребёнку (двойной sweep/decrement
  bug, не cross-round — проверить `nova_scope_sweep_dead_child`/
  `nova_scope_retain_or_release_child` на double-release).
- Изолировать пробу БЕЗ переиспользования стекового адреса `q` между
  раундами (напр. каждый раунд — отдельная функция/файбер) — если лосс
  исчезает, cross-round contamination подтверждена; если сохраняется —
  причина внутри ОДНОГО раунда.
- WSL/Linux сравнение (playbook §2.4) — Windows-specific?
- ARMED c NOVA_MAXPROCS=1 (single worker) — если лосс исчезает, подтверждает
  многопоточную природу (worker-vs-worker), не owner-vs-worker.

Инструментальный код (getenv-gated diag счётчики, write-log,
slot-collision-скан, SEQ_CST+sleep decisive-recheck) НЕ оставлен в дереве
(убран после использования, чтобы не грузить `nova_rt/` посторонним debug
scaffolding) — воспроизвести по этому README на новом окне за ~30-60 минут
(инструментация ловится в `nova_fiber_report_child_kinded` (fibers.h),
`nova_supervised_process_decisions` (fibers.h), точки `if (remote==0)
break;`/после `pending_sweeps` wait-loop в `nova_supervised_run_impl`).

## Окно p-vela2 (2026-08-03) — ROOT CAUSE НАЙДЕН И ПОЧИНЕН, статистика 90/90

**Опровергнута ГЛАВНАЯ гипотеза брифа (cross-round scope contamination через
переиспользуемый стековый адрес `q`).** Метод: та же проба, плюс
generation-инструментация (монотонный счётчик `_diag_gen` на
`NovaFiberQueue`, бампится в `nova_scope_init`; per-slot `_diag_alloc_gen`/
`_diag_write_gen` на `NovaChildError`) + write-log (`(q, gen, slot)` на
каждый `report_child_kinded`/`ALLOC`/`INIT`/decrement). Для КАЖДОГО
«пропавшего» слота, найденного точечным сканом `published` сразу после
`pending_remote==0 && pending_sweeps==0` (та же точка, что и в p-vela):
`alloc_gen == write_gen == текущее поколение` — НИКОГДА расхождения по
поколениям за все лоссовые прогоны. Полная построчная реконструкция лога
одного лоссового раунда (`q=...DE58, gen=12, slot=29`, воспроизведено ещё в
2 прогонах на РАЗНЫХ слотах/поколениях) показала: `REPORT` (запись
`published=true`) для этого слота РЕАЛЬНО произошёл, с правильным
поколением, **до** финального скана — но финальный скан всё равно увидел
`published==false`. Это возможно только если чтение и запись шли через
**разные физические массивы `child_error[]`** — то есть корень не в
`NovaFiberQueue`/поколениях вообще, а в дочернем heap-массиве.

**НАСТОЯЩИЙ ROOT CAUSE:** `nova_scope_grow_children` (fibers.h) —
вызывается ТОЛЬКО с owner-потока (из `nova_scope_alloc_child_slot`, пока
owner ещё в цикле `spawn`), удваивает `child_error[]`/`child_ctx[]`
(`NOVA_SCOPE_INITIAL_CAP=16` → 32 → 64 для `n=64`) и делает copy-then-swap
указателя **БЕЗ какой-либо синхронизации**: `scope->child_error = new_err;`
— голое, невыровненное присваивание. Одновременно worker-потоки для УЖЕ
завершившихся детей (заспавненных РАНЬШЕ, пока owner ещё продолжает
спавнить остальных 64 — типичная M:N-картина: быстрые дети финишируют,
пока owner ещё не вышел из цикла `for i in 0..n { spawn {...} }`) читают
`parent->child_error` (голый, невыровненный load) в
`nova_fiber_report_child_kinded` и пишут в `parent->child_error[slot]`.
Если worker прочитал указатель ДО swap'а (получил СТАРЫЙ массив), а owner
тем временем уже скопировал (ещё-неопубликованное) состояние этого слота в
НОВЫЙ массив и переключил указатель — запись worker'а уходит в
осиротевший старый массив; НИКТО больше туда не смотрит (`q->child_error`
теперь = новый массив). `pending_remote`/`pending_sweeps` — поля НА САМОЙ
`NovaFiberQueue` (не на реаллоцируемых массивах), поэтому считаются
идеально верно (объясняет `remote_inc==remote_dec==64` из p-vela) — гонка
целиком в перевыделяемых `child_error[]`/`child_ctx[]`, а не в scope или
его generation. Это же объясняет, почему SEQ_CST-барьер+5мс-сон (p-vela,
решающий эксперимент) не помог: `published` физически лежал в памяти,
которую никто больше не читал, — ждать было нечего.

**Фикс:** новое поле `nova_atomic_int child_lock` на `NovaFiberQueue`
(тот же spinlock-паттерн, что и существующий `slot_lock`, Plan 83.11
Ф.3.B) — держится (а) на всём copy+swap в `nova_scope_grow_children`, (б)
на read-pointer-then-write-slot секции в `nova_fiber_report_child_kinded`
(оба филиала: has_supervisor и default-path). Не трогает
`nova_scope_alloc_child_slot`'ную инициализацию НОВОГО слота (до него ни
один worker не может дотянуться — `_nova_parent_slot` раздаётся ребёнку
только ПОСЛЕ этого вызова) и ничего после `_drain_started` (грои туда
структурно не достают — R2-tripwire это уже доказывает, тут не менялось).
Не меняет гарантию D416§2 (сериализация decision-loop на drive-файбере) —
чинит ДРУГОЙ, нижележащий баг (retention-массив терял записи ДО того, как
decision-loop вообще успевал их увидеть); контракт спеки не тронут, спек-
амендмент не требуется.

**Статистика (`probe173_atomic_checker_clean.nv`, n=64×rounds=20, изолир.
`.exe`, без diag-инструментации):** база (это окно, ДО фикса, тот же
worktree) — **25/30 PASS**. После фикса — **30/30, 30/30, 30/30** (три
независимых батча подряд = 90/90). С diag-инструментацией (генерация +
write-log) — 10/10 без единого `MISSING` (до фикса — лоссы в ~1/3-1/5
прогонов). Диагностический код (generation-счётчики/write-log/EARLY-
RETURN/LOCAL-REPORT/THROW-логи) использован для локализации и **убран из
дерева** тем же порядком, что и в p-vela — итоговый diff чисто аддитивен,
+80 строк, только `nova_atomic_int child_lock` + два spinlock-участка
(`git diff --stat`: 1 файл, `compiler-codegen/nova_rt/fibers.h`,
`80 insertions(+)`, `emit_c.rs`/`effects.h` возвращены byte-identical).

**Гейты (это окно, worktree `nova-vela2`, ветка `pvela2`, НЕ влита):**
`cargo build --release` (nova-codegen + nova-cli) — чисто, только
pre-existing warnings (0 новых, `git status` подтверждает: изменён только
`fibers.h`). `nova check std/src` — **PASS 147 / FAIL 26 / WARN 60** —
канон совпал. polaris `nova.exe test src --strict-effects` — **PASS 37 /
FAIL 0 / SKIP 18** — канон совпал. `scripts/guards/arch-ratchet.sh` —
`lines=64505 <= 64505`, `infer=348 <= 348` — не сдвинуто (fibers.h не
входит в ratchet-скоуп). Мега-CU / флагман — НЕ прогонялись этим окном
(интегратор при приёмке, per бриф).

**Рекомендованные следующие шаги для интегратора (НЕ выполнены этим окном
— вне мандата «починить рантайм», плюс `child_lock` лучше сначала
прогнать под мега-CU/флагманом лично интегратору перед раскруткой):**
1. Слить `pvela2` (или cherry-pick `child_lock`-диффа) после личной
   проверки владельца (M:N-безопасность — контролируемая зона).
2. Вернуть carve-out Supervisor в энфорс D441 (отозван окном 238/A-V10),
   раз D416§2 рантаймом теперь реально выполняется.
3. Вернуть `supervisor_on_child_fail_serialized_pin_test.nv` в
   conformance как pin-регресс (сейчас существует только как
   `probe173_atomic_checker_clean.nv`, не в реестре).
4. Снять пометку «нарушен» с D416§2 в `spec/decisions/06-concurrency.md`
   (D-текст самого контракта не менялся — обновляется только статус
   реализации).
5. Смигрировать обратно на mut-захват polaris-сайты, если это было
   единственной причиной их миграции на Atomic (проверить).
6. Опционально (производительность, не корректность): `child_lock`
   сериализует ВСЕ завершения детей в scope друг с другом (не только
   против grow) — при очень широком fan-out это лишний contention;
   ре-дизайн на chunked/stable-address storage для `child_error[]`
   (готовый fallback, уже упомянут в комментарии R2-tripwire у
   `nova_scope_grow_children` как "Option A") убрал бы grow вообще, а с
   ним и саму необходимость в `child_lock` — но это отдельная волна,
   не блокер для текущего фикса.
