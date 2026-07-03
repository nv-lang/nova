# Roadmap 173–181 — рекомендованный порядок исполнения (2026-07-04)

**Автор:** автономная сессия по запросу владельца («выполни план в рекомендуемом тобой
порядке в фоне»). **Дисциплина:** 172.1-класс — production-grade, без упрощений; spec-first
(D/Q); pos+neg тесты через релизный nova (C-codegen); полный nova_tests baseline на каждый
атом; коммит после каждой большой задачи; конвенции compiler-/test-/nv-coding-style/module.
**Baseline-сверка:** temp-worktree / commit-reset — НЕ git stash (repo-global).

## Порядок (dependency-driven критический путь)

```
173.0  concurrency runtime substrate      [ГЕЙТ для 173]
  ↓
173    error system unify + hardening      [РАЗБЛОКИРУЕТ 174, 176]
  ↓
{174   lang & FFI features}  {176  I/O + FS + OS}
  ↓
175    time system            177  fallible-Result (уже IN PROGRESS — довести)
  ↓
178 std/http · 179 encoding/compress · 180 serde/derive · 181 same-scope rebinding
```

**Обоснование:** 173 — keystone: блокирует 174 (FFI) и 176 (I/O). 173.0 — его runtime-гейт
(structured-concurrency субстрат). Закрыв 173-семью, разблокируем наибольшее число downstream.
175/177 идут параллельно (независимы). 178–181 — прикладные, поверх разблокированного ядра.

## Статус исполнения (обновляется по ходу)

| План | Статус | Примечание |
|------|--------|-----------|
| 173.0 | 🔨 | старт |
| 173   | ⏳ | после 173.0 |
| 174   | ⏳ | после 173 |
| 176   | ⏳ | после 173 |
| 175   | ⏳ | параллельно |
| 177   | ⏳ | довести (IN PROGRESS) |
| 178–181 | ⏳ | прикладные |

## Критерии приёмки (общие, на каждый план)

1. «Без упрощений как для прода» — **обязательный** критерий (spec-first, полный функционал).
2. Соответствие compiler-conventions §0/§1/§2 + test-conventions + nv-coding-style + module-conventions.
3. Pos+neg тесты через релизный nova (C-codegen), не интерпретатор; spec_tests/conformance (D-блок).
4. 0 регрессий против чистого бинаря (полный nova_tests, не сэмпл).
5. Спека/D/Q/docs обновлены по факту; project-creation.txt + simplifications.md + backlog + discussion-log.
