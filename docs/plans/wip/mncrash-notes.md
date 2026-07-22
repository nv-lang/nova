<!-- SPDX-License-Identifier: CC-BY-4.0 -->
# [M-conformance-megacu-intermittent-run-crash] — расследование, чекпоинт

Worktree: `C:/Users/Public/nova-mncrash` (НЕ `d:/../nova-mncrash`: диск D —
exFAT с кластером 1 МБ, checkout 4516 файлов требует ~4.5 ГБ при 47 МБ
контента, диск заполнен на 100% данными владельца → worktree на C:/NTFS;
первая попытка `C:/Users/<кириллица>` ломала cl.exe кодировкой путей),
ветка `p-fix-mn-crash`, база main @ 36380881c. Модель: fable.

## Симптом (из маркера)

Мега-CU `nova test --positive --compile-error --timeout 600 --jobs 16
spec_tests/conformance` изредка (~1/8) даёт RUN-FAIL на entry
`a_q3_println_debug_record`: elapsed ~517s, exit≠0, ни одной `FAIL:`/`panic:`
строки — процесс молча умирает mid-run.

## Репро — НАЙДЕН, дёшев и стабилен

1. Гейт полный (worktree-сборка): PASS 528/0/55; entry 272.9s (соло-машина).
   Соло-прогон mega-exe напрямую: **4.2s** (2818 assert'ов) — т.е. 517s из
   маркера были почти целиком КОМПИЛЯЦИЕЙ под нагрузкой 16 jobs; RUN-фаза
   коротка (playbook §2.1.0 — «сначала прогони exe напрямую» подтверждён).
2. **Прямой репро**: 4 параллельные копии mega-exe (разные cwd) ×10 раундов =
   **6 падений / 40 прогонов (p≈15%)**, exit=3 (abort после VEH-дампа).
   Соло — 0 падений (нужна конкуренция за CPU → задержка орфана).

## Корень — НАЙДЕН (VEH+dbghelp, NOVA_DIAG_SEGV=1, frame[1] за 1 прогон)

Все 6/6 крахов — ОДНА сигнатура:

```
#00 Nova_AtomicInt_method_fetch_sub_int+0x25 (nova_rt/sync_primitives.h:630)  AV-WRITE
#01 _nova_detach_1+0xAF0 (a_q3_println_debug_record.c:176527)   ← KEYSTONE
#02 _mco_main (minicoro.h:623)
```

Генерированный C (фикстура `spec_tests/conformance/detach_consume_move_ok.nv`,
добавлена 2026-07-22 08:34 коммитом 5065f684d, волна p-fix-detach-consume —
т.е. крашится гейт ПОСЛЕ её появления; краш 2026-07-13 на
`app_effect_basic_t8_1` — тот же класс, но другой инстанс/фикстура):

```c
static nova_unit nova_test_detach_consume_stream...(void) {
    Nova_AtomicInt* inflight = Nova_AtomicInt_static_new(0);  /* СТЕК-ЛОКАЛЬ */
    ...
    _nova_detach_1_ctx->inflight = &inflight;   /* ← АДРЕС СТЕКОВОЙ ЛОКАЛИ в ctx ОРФАНА */
    nova_runtime_spawn_orphan(_nova_detach_1, _nova_detach_1_ctx);
    return NOVA_UNIT;                            /* кадр умер; орфан живёт */
}
/* тело орфана: */
... = Nova_AtomicInt_method_fetch_sub_int((*_c->inflight), 1);  /* USE-AFTER-RETURN */
```

**Это НЕ гонка планировщика/GC** (три TSan-гонки 211 §7.3/§7.4 закрыты и
присутствуют в main — проверено кодом). Это **codegen-баг emit_detach**
(`compiler-codegen/src/codegen/emit_c.rs`): capture-анализ detach
(`let by_value = !is_mut;`) шлёт mut-капчеры **by-reference** — скопировано
с emit_spawn, где это корректно (supervised join'ится ДО выхода кадра). Для
detach (fire-and-forget орфан, переживает кадр) by-ref капчер стековой
локали = гарантированный use-after-return; окно = задержка старта орфана
vs реюз стека родительского файбера (под нагрузкой окно расширяется —
отсюда load-sensitivity и «1 из 8 прогонов гейта»). Нарушение §9
mn-coding-conventions (стек-указатель за границей потока), но фикс не
counter-wait (detach по построению не ждёт), а **heap-box капчера**.

## Дизайн фикса (реализуется)

Семантика по спеке D50 §3.1: `mut x = 0; detach { x = 42 };
runtime.drain_orphans(); assert(x == 42)` — мутация ДОЛЖНА быть видна
родителю ⇒ снапшот by-value недостаточен. Эталон (Go): escape analysis →
переменная уезжает в кучу. У нас уже есть идиома —
`emit_effect_handler_literal` case (a) «escaping handler»: ленивый
heap-box + регистрация в `var_boxed` (последующие чтения/записи имени в
объемлющей функции прозрачно деref'ят бокс).

emit_detach call-site, для `!by_value` (mut) капчеров, вместо
`ctx->cap = &cap;`:
1. если `var_boxed[cap]` уже есть — реюз существующего бокса;
2. иначе `T* _nv_dbox<N>_<cap> = nova_alloc(sizeof(T)); *box = <тек.значение>;`
   + `var_boxed.insert(cap, box)`;
3. `ctx->cap = box;` (тип поля ctx НЕ меняется — тот же `T*`, меняется
   источник: стек → GC-куча; тело орфана `(*_c->cap)` не трогаем).

Свойства: бокс collectable (nova_alloc), достижим через ctx (uncollectable,
сканируется) → жив, пока жив орфан; заодно чинит потенциальный GC-losing
самого AtomicInt после смерти родительского кадра. D415-шный
E_CONCURRENT_MUT_CAPTURE и так пропускает через detach только #share-типы
(AtomicInt/Mutex/#share-рекорды) — box-копия хэндла сохраняет
shared-мутацию через кучу; rebinding-видимость (`x = 42`) даёт var_boxed
для линейного кода (канонический паттерн спеки). Известные принятые
ограничения (комментируются в коде): loop-carried rebinding-видимость
scalar-капчера до detach-строки внутри цикла (эмиссионный порядок), реюз
бокса между несколькими detach одного имени в сиблинг-C-блоках — тот же
класс, что уже документирован у handler'ов
([M-175-handler-lit-boxed-var-c-scope-leak]).

## Верификация (план)

p=0.15 → n ≥ ⌈-ln(0.01)/0.15⌉ = 31; беру 40+ прямых прогонов (4-way ×10+)
чистыми + полный гейт --jobs 16 ×10 + detach_effect_ok_test/
share_capture_ok_test/std concurrency + флагман-сборка.
