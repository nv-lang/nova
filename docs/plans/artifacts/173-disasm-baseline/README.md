# Ф.2.A0 — disasm/структурный baseline (Cleanup[Never] hot-path)

> Plan 173 Ф.2 (defer-kernel). Baseline-референс для **PARITY**-acceptance (§3.5, D314 §5):
> lowered consume/defer(o) обязан давать вывод ≡ дорефакторному frame-bearing. Снят на старте
> Ф.2 (commit-родитель `7c7ca63d`).

## 🔴 Ключевой факт (де-риск Ф.2, 2 агента независимо)

**§perf-элизия D194 НЕ реализована.** Единственная эмитящая ConsumeScope-ветка
(`emit_c.rs:19746-20031`) эмитит ПОЛНЫЙ frame-bearing путь **БЕЗУСЛОВНО** — нет Never/infallible-ветки,
grep effect-row-inspection в emit_c = 0. Поэтому Ф.2-acceptance = **PARITY** (не регрессировать этот
frame-bearing вывод), а НЕ «сохранить существующую элизию» (её нет). Генуинная §perf-элизия →
followup `[M-173-d194-perf-elision]`.

## Дорефакторная структура consume-lowering (из рекона `emit_c.rs:19746-20031`)

Каждый `consume X = e { body }` с Consumable[Never]-guard (Mutex/Read/Write/Permit) эмитит:
- `nv_consume_enter_shield(timeout)` / `nv_consume_leave_shield(prev)` — БЕЗУСЛОВНАЯ пара (19831/19980);
- 3-level exit-timeout resolution (19773-19820);
- `if(_nova_handler_Cleanup) Nova_Cleanup_on_scope_enter/exit(...)` — ResourceTrace-dispatch (19843/19967);
- **body fail-frame** — `NovaFailFrame`+`setjmp` (19850);
- **on_exit fail-frame** — ВТОРОЙ `NovaFailFrame`+`setjmp` (19938);
- 4 `nova_make_ScopeOutcome_*` (Success/Cancel→Failure/Failure/Panic, 19891-19926);
- 6-way re-raise ladder (19992-20024).
⇒ **≥2 setjmp-кадра** на consume-block (body + on_exit), shield-пара, полный outcome. Это и есть
parity-таргет: после B3 (consume→defer(o)-desugar) число кадров/shield/outcome НЕ должно вырасти.

## Статус фикстур на старте Ф.2 (7c7ca63d)

- `nova_tests/plan103_9/mutexguard_basic_unlock.nv` — **PASS** (компилится+проходит).
  Hot-path guard MutexGuard consume-block через ту же ветку 19746.
- disasm-guard targets (byte-identical acceptance): MutexGuard/ReadGuard/WriteGuard/Permit
  (`std/runtime/sync.nv:1390/1402/1414/1428`), atomics.

## План точного захвата

Реальный byte-level .c/disasm diff-baseline снимается **прямо перед Ф.2.B3** (consume→defer(o)-desugar),
ПОСЛЕ ренеймов R1/R2 (иначе rename-шум `Cleanup→ResourceTrace`/`on_exit→cleanup` забьёт структурный diff).
Тогда: `nova build`/`--keep-artifacts` → сохранить `.c` MutexGuard-фикстуры сюда как `mutexguard.pre-b3.c`,
и diff'ить против post-B3. Захват сейчас (pre-rename) был бы устаревшим референсом.
