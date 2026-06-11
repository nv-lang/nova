<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# Plan 147 — Pointer mutability: running-current flip-scan model (reverse 138.5 §V2)

> **Создан:** 2026-06-11.  **Статус:** 📋 PLANNED (decomposition).  **Worktree:** `nova-p138` @ `plan-138.1`.
> **Model:** Opus + Thinking ON (type-system / parser / checker semantics).
> **Триггер:** дизайн-ревью пользователя (2026-06-11) — текущая 138.5 §V2 модель
> (independent axes, `*T ≡ *ro T`, явный `*mut T` обязателен) **неверна**. Правильная модель —
> **running-current flip-scan**: `ro`/`mut` распространяется вправо, postfix-модификатор только
> **переворачивает** current, совпадение → избыточно (ошибка).
> **Production, без упрощений.** Гейтит Plan 139 `[M-139-f0-lang-item-decl]` (str-поля под этой моделью).

---

## Модель (running-current flip-scan)

Объявление указателя: `<binding> name <type>`, где `<type>` — цепочка `* [mod] * [mod] … [mod] T`.

**Правила:**
1. **applies-right.** Модификатор `ro`/`mut` управляет тем, что **непосредственно справа** от него
   (`*` → мутабельность указателя на этом уровне; тип `T` → мутабельность pointee).
2. **binding = внешний `*`.** Мутабельность самого внешнего `*` пишется как **binding** (`ro`/`mut`
   перед именем переменной/поля), из типа убирается. Задаёт **начальный `current`**.
3. **flip-scan.** Сканируя тип слева→направо, поддерживается **`current` мутабельность**, которая
   **распространяется вправо**. Каждый **postfix** модификатор (после `*`) обязан **перевернуть**
   `current` (`ro`↔`mut`); после этого новый `current` распространяется до следующего flip.
   Финальный pointee `T` берёт текущий `current`.
4. **redundant → ошибка.** Postfix-модификатор, **совпадающий** с `current` (не переворачивает) →
   **`E_REDUNDANT_POINTER_MODIFIER`** (одна каноническая форма; «one canonical syntax»).
5. **prefix → ошибка.** Модификатор **перед** `*` → **`E_POINTER_PREFIX_MODIFIER`**: указатель —
   простой тип без доступных для записи полей, модификатор к нему неприменим; его мутабельность
   (reassignability) берётся из **binding**. Связь: `[M-138-binding-type-mut-conflict]` (тот же
   принцип «модификатор применим только там, где есть что мутировать»).

**Эталонная таблица:**
| Запись | current-сканирование | Вердикт |
|---|---|---|
| `mut data *T` | binding mut → T=mut (нет flip, явного нет) | ✅ T мутабельно |
| `mut data *mut T` | binding mut; `*mut` совпадает → нет flip | ❌ `E_REDUNDANT_POINTER_MODIFIER` |
| `mut data *ro T` | binding mut; `*ro` flip→ro; T=ro | ✅ override |
| `ro data *mut T` | binding ro; `*mut` flip→mut; T=mut | ✅ фикс-указатель, writable target |
| `ro data *mut *ro T` | ro → `*mut` flip→mut → `*ro` flip→ro; T=ro | ✅ double-ptr (внеш ro, внутр mut, T ro) |
| `ro data mut *T` | `mut` перед `*` (prefix) | ❌ `E_POINTER_PREFIX_MODIFIER` |

**Каноничность:** `*T` = «current без изменения» (наследует). Явный `*mut`/`*ro` пишется **только**
когда переворачивает. Это убирает дубли (`mut data *mut T`) и даёт единственную форму на каждый смысл.

---

## Разворот 138.5 §V2 (что меняется в спеке)

- **`*T ≢ *ro T`.** Старое `*T ≡ *ro T` (always-ro pointee, 02-types.md:7538) **ОТМЕНЕНО**: `*T`
  наследует `current` (mut-binding→mut, ro-binding→ro).
- 02-types.md pointer-binding таблица (7531-7539) + §V2.6 + vec_owned.nv:78-97 комментарий —
  переписать под flip-scan.
- D216 §V2 «independent axes / postfix-only-explicit» — заменить на flip-scan (новый D-block D245).

---

## Фазы (атомарные; production; per-phase commit; pos+neg tests релизным nova)

### Ф.1 — D-block D245 + спека
- Новый **D245** (03-syntax.md или 02-types.md) — flip-scan модель: 5 правил + эталонная таблица +
  applies-right + binding=outermost + E_REDUNDANT/E_PREFIX. Amend/retract D216 §V2.6.
- Переписать pointer-binding таблицу (02-types.md:7531-7539) + `*T` семантику (`*T` наследует current).
- README D-index + per-file TOC. **Commit:** `spec(plan147 Ф.1 D245): pointer mut flip-scan model; reverse D216 §V2`.

### Ф.2 — parser: applies-right + binding-hoist
- `parse_type` Star-arm (parser/mod.rs:5214): принимать postfix-модификаторы как flip; запретить prefix
  (`E_POINTER_PREFIX_MODIFIER` уже есть — выверить под новую rationale). Сохранять `current` при рекурсии.
- AST несёт per-level explicit-mod (для checker redundancy/flip-проверки).
- **Commit:** `feat(plan147 Ф.2): parser flip-scan pointer modifiers (applies-right, binding-hoist)`.

### Ф.3 — checker: flip-validation + E_REDUNDANT
- Валидация цепочки: postfix должен flip'ать current; совпадение → `E_REDUNDANT_POINTER_MODIFIER`
  (с fix-it «убрать избыточный `*mut`/`*ro`»). Pointee-mut вычисляется из flip-scan (для проверки
  записи `*p=…`/`@data[i]=…`). binding-default корректно.
- **Commit:** `feat(plan147 Ф.3): checker flip-validation + E_REDUNDANT_POINTER_MODIFIER`.

### Ф.4 — миграция codebase
- Сканировать все pointer-декларации (поля/локалы/параметры). Привести к flip-scan-каноне:
  - Vec `mut data *mut T` → **`mut data *T`** (vec_owned.nv:97/750); обновить комментарий 78-97.
  - Все избыточные `*mut`/`*ro` (совпадающие с binding) → убрать.
  - prefix-формы (если остались) → починить.
- Broad-регрессия 0 new FAIL. **Commit(s):** per-кластер.

### Ф.5 — pos/neg tests (релизным nova)
- `nova_tests/plan147/`: pos (`mut data *T`, `mut data *ro T`, `ro data *mut T`, `ro data *mut *ro T`),
  neg (`mut data *mut T`→E_REDUNDANT, `ro data mut *T`→E_PREFIX, redundant в double-ptr).
- **Commit:** `test(plan147 Ф.5): flip-scan pos/neg corpus`.

### Ф.6 — docs + close
- project-creation + simplifications + backlog (`[M-138-binding-type-mut-conflict]` cross-ref;
  убрать stale 138.5-формулировки) + nova-private/discussion-log. Acceptance audit. Push.
- **Гейт снят для** Plan 139 `[M-139-f0-lang-item-decl]` (str-поля под flip-scan: `ptr *u8`).

---

## Acceptance
- **A1** — 6 эталонных форм: pos компилируются, neg дают E_REDUNDANT/E_PREFIX. plan147 GREEN.
- **A2** — `*T` наследует binding-current (mut-binding→writable pointee; ro→ro). Доказано pos-фикстурой записи.
- **A3** — Vec `mut data *T` (после миграции) — push/index-set работают (pointee mut via inherit). plan138_* GREEN.
- **A4** — D245 + спека (pointer-таблица + `*T` семантика) переписаны; D216 §V2.6 retracted/amended.
- **A5** — 0 регрессий vs baseline; broad pointer-using dirs (plan115/118/138/128) clean.

## Risks
| # | Риск | Sev | Mitigation |
|---|---|---|---|
| R1 | Разворот свежей 138.5 — много pointer-сайтов | 🔴 HIGH | Ф.4 broad-скан + регрессия; per-кластер commit |
| R2 | flip-scan в multi-level parser/checker — тонко | 🟡 MED | эталонная таблица как тест-оракул; double-ptr фикстуры |
| R3 | str-поля зависят (gating) | 🟡 MED | Ф.6 снимает гейт; 139 lang-item следом |

## Связь
- Разворачивает **D216 §V2.6** (02-types.md:8195) + pointer-таблицу (7531-7539).
- `[M-138-binding-type-mut-conflict]` — тот же принцип (модификатор применим где есть mutable-начинка).
- Гейтит **Plan 139 `[M-139-f0-lang-item-decl]`** (str `type str value priv {ptr *u8, len int}` под flip-scan).
- `[M-138-double-pointer-codegen-test]` — multi-level pointer (этот план уточняет модель).
