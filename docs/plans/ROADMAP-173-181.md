# Roadmap: открытый хвост 172 + планы 173–181 — порядок исполнения (2026-07-04)

> **UPD 2026-07-04:** по замечанию владельца включены открытые под-планы зонта 172
> (ранее roadmap покрывал только 173–181). Ядро 172 (172.1-172.4-core) закрыто; ниже — хвост.

**Автор:** автономная сессия по запросу владельца («выполни план в рекомендуемом тобой
порядке в фоне»). **Дисциплина:** 172.1-класс — production-grade, без упрощений; spec-first
(D/Q); pos+neg тесты через релизный nova (C-codegen); полный nova_tests baseline на каждый
атом; коммит после каждой большой задачи; конвенции compiler-/test-/nv-coding-style/module.
**Baseline-сверка:** temp-worktree / commit-reset — НЕ git stash (repo-global).

## Порядок (два трека)

**ТРЕК A — архитектурный фундамент 172 (front-load: делает весь codegen + 173-181 чище):**
```
172.12  typed-IR mono          [убивает C-строковую identity + lifted-arms-долг]
  ↓
172.13  constraint-inference   [унификационное ядро поверх typed-IR]
172.5   in-out ref-params      [standalone фича, независима — по мере готовности]
```

**ТРЕК B — критический путь фич 173–181 (dependency-driven):**
```
173.0  concurrency runtime substrate   [ГЕЙТ для 173]
  ↓
173    error system unify + hardening   [РАЗБЛОКИРУЕТ 174, 176]
  ↓
{174 lang & FFI}  {176 I/O + FS + OS}
  ↓
175 time · 177 fallible-Result (IN PROGRESS — довести) · 178 http · 179 encoding · 180 serde · 181 rebinding
```

**Отложено (решение владельца):** 172.14 (value-ABI perf P3) — не в текущем окне.

**Обоснование порядка:**
- Трек A (172.12 фундамент) — front-load: фичи 173-181 поверх typed-IR = меньше техдолга;
  172.12 — крупнейший архитектурный разрыв с эталонами (rustc/Zig/Swift), окупается на всём последующем.
- Трек B (173 keystone) — блокирует 174 (FFI) и 176 (I/O); 173.0 — его runtime-гейт.
- Треки МОГУТ идти параллельно (A — checker/codegen; B стартует с runtime-субстрата 173.0).
- ФАКТИЧЕСКИЙ старт (2026-07-04): 173.0 de-risk уже запущен (гейт нужен в любом порядке);
  172.12 подключается как параллельный архитектурный трек.

## Статус исполнения (обновляется по ходу)

| План | Трек | Статус | Примечание |
|------|------|--------|-----------|
| 172.12 | A | ⏳ | typed-IR mono (фундамент) |
| 172.13 | A | ⏳ | после 172.12 |
| 172.5  | A | ⏳ | in-out ref, standalone |
| 173.0  | B | 🟡 | Ф.1 drain-race ЗАКРЫТ (deliverable: spec+guard, 58aca50b); Ф.2/Ф.3 supervised-substrate — deep runtime, секвенс с 173.2 (его потребитель) |
| 173    | B | 🔨 | **Ф.1 ЗАКРЫТА** (#1/#2/#4/#3/#7). **Ф.2 defer-kernel В РАБОТЕ:** Ф.2.0 D314-spec + де-риск-карта закрыты (c625808e; 🔴 D194-премиса ложна→parity+followup, nova_scope_exit policy, rename-collision, interrupt→Failure). Реализация: A0→R1-ResourceTrace→R2-Consumable/Cleanup→B1/B2 defer(o)→B3 consume-desugar→C nova_scope_exit→D194→E-hub |
| 174    | B | ⏳ | после 173 |
| 176    | B | ⏳ | после 173 |
| 175    | B | ⏳ | параллельно |
| 177    | B | ⏳ | довести (IN PROGRESS) |
| 178–181 | B | ⏳ | прикладные |
| 172.14 | — | ⏸ | P3, отложен владельцем |

## Критерии приёмки (общие, на каждый план)

1. «Без упрощений как для прода» — **обязательный** критерий (spec-first, полный функционал).
2. Соответствие compiler-conventions §0/§1/§2 + test-conventions + nv-coding-style + module-conventions.
3. Pos+neg тесты через релизный nova (C-codegen), не интерпретатор; spec_tests/conformance (D-блок).
4. 0 регрессий против чистого бинаря (полный nova_tests, не сэмпл).
5. Спека/D/Q/docs обновлены по факту; project-creation.txt + simplifications.md + backlog + discussion-log.
