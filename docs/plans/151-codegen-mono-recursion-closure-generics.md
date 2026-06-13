<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# Plan 151 — Codegen: unbounded mono-recursion в обобщённых функциях с closure-параметрами (`within[T]`/`race2[T]`)

> **Создан:** 2026-06-13 (из Plan 149 fu #1 / marker `[M-cancellation-test-mono-recursion-overflow]`).
> **Статус:** 📋 PLANNED — **audit-first** (форма фикса зависит от root-cause).
> **Приоритет:** P2 — конкретный generic-паттерн (не вся мономорфизация), не блокер релиза; но
> чинит реальный codegen-баг + un-quarantine'ит `cancellation_test`.
> **Оценка:** ~1-2 dev-day (audit может растянуть; фикс может быть мал).
> **Родитель:** marker `[M-cancellation-test-mono-recursion-overflow]` (home: Plan 83.4.5.10 / codegen),
> Plan 149 fu #1 (quarantine). Инфра: [Plan 48](48-closures-in-generics.md) / [Plan 55](55-codegen-followups-from-plan-54.md) (мономорфизация).
> **Worktree:** `nova-p151` · ветка `plan-151`.

---

## 1. Проблема

Обобщённые функции `within[T]` / `race2[T]` (хелперы отмены в `std/concurrency/cancellation.nv`,
берут closure-параметры `fn() -> T`) при мономорфизации генерируют C-код с **UNBOUNDED (бесконечной)
рекурсией** → на рантайме fiber переполняет стек («fiber stack overflow in slot 0»).

**Ключевое:** это **НЕ глубина** (бо́льший стек бы вылечил), а **runaway** → никакой размер не
помогает (проверено Plan 149 fu #1: и `NOVA_FIBER_STACK=64MB`, и одиночный `within[T]` в своём TU —
всё равно overflow). **НЕ stack-size issue.** Pre-existing: помечен в Plan 83.4.5.10 как «crash @1MB»;
codegen с тех пор дрейфанул (теперь overflow и на 8MB). Всплыл при broad-верификации Plan 149 →
`cancellation_test` отправлен в quarantine (`nova_tests/concurrency/cancellation_quarantine/`).

## 2. Гипотезы (audit решит)
- **(a) Потерянный base-case** — рекурсивный lowering `within[T]`/`race2[T]` потерял условие остановки.
- **(b) Бесконечная mono-цепочка** — инстанцирование `within[T]` триггерит инстанцирование
  `within[T']`/вложенного без сходимости (compile-time mono-loop).
- **(c) Closure-param lowering** — генерация self-referential вызовов для `fn() -> T` аргументов.
- **Локусы:** `compiler-codegen/src/const_fn_mono.rs` (mono-пасс), `compiler-codegen/src/codegen/emit_c.rs`
  (lowering обобщённых типов + closures; ~mono naming ~11737).
- **Артефакт:** сгенерённый `cancellation_test.c` (within[int] ~4733-4782, within[str] ~4817-4900).

## 3. Фазы

### Ф.0 — GATE: audit (research-first)
- Дизассемблировать сгенерённый C для `within[int]`/`race2[int]` (минимальный repro — один `within[int]`).
- Определить: **compile-time** mono-loop (компилятор зацикливается/раздувает) **vs runtime** call-depth
  (сгенерённый код рекурсит без base-case). Замерить глубину/паттерн.
- Регрессия: что изменилось между Plan 83.4.5.10 (crash@1MB) и сейчас (overflow@8MB) — git-bisect/diff
  релевантных mono/emit изменений.
- **Решение:** какая гипотеза (a/b/c) + форма минимального фикса. Если root-cause фундаментален
  (дизайн мономорфизации) — честно: точный диагноз + scoped-фикс ИЛИ deeper-план, НЕ фейк-фикс.

### Ф.1 — Codegen fix
- Реализовать минимальный фикс по Ф.0 (восстановить base-case / терминировать mono-цепочку /
  поправить closure-lowering) в `const_fn_mono.rs` / `emit_c.rs`. cargo build --release.

### Ф.2 — Un-quarantine
- Убрать `nova_tests/concurrency/cancellation_quarantine/_fixture.toml`, вернуть `cancellation_test.nv`
  в `nova_tests/concurrency/` (модуль обратно `concurrency.cancellation_test`, D78). Verify PASS @4MB default.

### Ф.3 — Broad regression (ОБЯЗАТЕЛЬНО)
- Мономорфизация **глобальна** → прогнать **полный** test-suite (не только cancellation), особенно
  generic-heavy (plan48/plan55/plan59/plan101 + generics fixtures). Любой regress → fix перед закрытием.

### Ф.4 — Позитивные + негативные тесты
- POS: `within[int]`/`within[string]`/`within[bool]`/`race2[int]` — все компилируются+бегут (no overflow).
- NEG/guard: фикстура, которая бы overflow'ила ДО фикса (regression guard на этот паттерн).
- Через релизную nova & компилятор.

### Ф.5 — Closure
- Spec/D/Q если фикс меняет контракт мономорфизации (вероятно нет — bugfix). Доки если уместно.
- **Закрыть marker** `[M-cancellation-test-mono-recursion-overflow]`. Acceptance §4.
- project-creation + simplifications + backlog + nova-private discussion-log. Merge + push.

## 4. Критерии приёмки
1. Root-cause найден + задокументирован (audit Ф.0).
2. Codegen-фикс реализован; clang/MSVC собирается.
3. `cancellation_test` un-quarantined + **PASS @4MB default** (через релизную nova).
4. `within[T]`/`race2[T]` для int/str/bool — все green.
5. **Полный suite no-regression** (мономорфизация-global).
6. Pos + neg/regression-guard тесты green.
7. Marker `[M-cancellation-test-mono-recursion-overflow]` закрыт.
8. Если root-cause глубже scope → честный диагноз + scoped-фикс/deeper-план, НЕ фейк.

## 5. Риски
- **Мономорфизация глобальна** → фикс может задеть любой generic-код → broad regression (Ф.3) критичен.
- Audit может показать фундаментальную проблему дизайна → тогда честный частичный фикс + diagnosis.
- Не фейк-фиксить (не удалять/выхолащивать тест; marker закрывать только при реальном фиксе).

## 6. Связь
- marker `[M-cancellation-test-mono-recursion-overflow]` (этот план — его дом).
- Plan 149 fu #1 — quarantine (этот план её снимает).
- Plan 48 / 55 / 59 — мономорфизация infra (где искать).
