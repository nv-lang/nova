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
