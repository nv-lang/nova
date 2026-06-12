<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# Plan 147 — Three-axis mutability model (supersede flip-scan / D245)

> **Создан:** 2026-06-11. **Reframed:** 2026-06-12 (flip-scan → 3-axis).
> **Статус:** 📋 PLANNED (decomposition). **Worktree:** `nova-p138` @ `plan-138.1`.
> **Model:** Opus + Thinking ON. **Production, без упрощений.**
> **История:** Ф.1-черновик flip-scan (D245, commit `befe92c`) **ОТКЛОНЁН** adversarial-критикой
> (4 BLOCKER: `*T` контекстно-зависим → тип не самодостаточен). Две design-workflow + ~15 раундов
> ревью с пользователем сошлись на **3-осевой модели** (L1 binding / L2 view-over-owned-graph /
> L3 pointee-capability-in-type). Этот план её формализует + откатывает flip-scan.
> **Снимает блокер** Plan 139 `[M-139-f0-lang-item-decl]` (str под чистой моделью).

---

## Модель: ТРИ ортогональные оси (каждая самодостаточна — C1)

| Ось | Что задаёт | Синтаксис |
|---|---|---|
| **L1 — binding** | переприсваиваемость **имени** + корень прав на запись через имя | `ro`/`mut` **перед именем** (никогда в типе) |
| **L2 — view** | транзитивная ro/rw по **owned-графу** значения (`.field`/`[i]`); **СТЕНА на каждом `*`** | `ro`/`mut` **перед типом** (value/record) |
| **L3 — pointee-capability** | можно ли писать **за `*`**; реально В ТИПЕ, позиционно-независимо | **постфикс**: `*T`(ro) / `*mut T`(mut) |

**Принцип (1 строка):** `ro` — дефолт везде; пишется только опт-ин (`mut x`, `mut T`, `*mut T`).
L2 транзитивно морозит owned-граф и **упирается в стену на `*`**; за указателем — только L3 из типа.
Soundness в GC (нет borrow-checker, aliasing): `ro` = «**это имя** не пишет», НЕ «объект заморожен».

### Канон синтаксиса
- `*T` = ro-pointee (канон, дефолт). `*mut T` = mut-pointee (единственный опт-ин).
- **`*ro T` → HARD ERROR** `E_REDUNDANT_POINTER_RO` («избыточно → используй `*T`»). (Выбор (a): потребителей мало, std в формировании.)
- **`mut *T` / `ro *T` (prefix) → `E_POINTER_PREFIX_MODIFIER`** (модификатор на `*` запрещён; reassign = binding).
- `ro T`/`mut T` (перед типом value/record) = L2 content-view. `ro x ro T`/`*ro ro T` → `E_REDUNDANT_TYPE_MODIFIER`.
- `**T ≡ *(*T)`, дефолт ro вниз; mut-уровни — явный `*mut` на нужном уровне (`*mut *T`, `**mut T`).
- **Откат D245:** `*T ≡ *ro T` **универсально**, во ВСЕХ позициях (param/return/generic/alias/cast/field/local). НЕТ наследования pointee-mut от binding (`current`/flip-scan). НЕТ cast-исключения (`x as *T`, не `as *ro T`).

### Дефолты
binding — пишешь явно `ro`/`mut`; **параметр** ro (D176); **return** mut-binding у caller'а (D184 — свойство binding, не значения); **pointee** ro (`*T`); **поле** mutable-у-mut-binding (D175).

### R1 vs R2 (разрешено — обе живут на разных осях)
- **R1 (transitive-ro)** = закон L2: `-> ro Value`/`-> ro HeapValue` морозят owned-граф (стена на `*`).
- **R2-split** = явный opt-in пары (L1,L2): `ro r mut Point` (reassign❌/content✅), `mut r ro Point` (reassign✅/content❌). **Голый `ro r` = freeze** (binding-dominates, P7); mut-content при фикс-имени → явный `mut`.
- **Coercion** = по оси content (L2): ro-источник → mut-content-цель = `E_READONLY_COERCE`; → ro-цель = OK. L1 (`ro a`/`mut a`) caller выбирает свободно. `*mut T → *T` авто-сужение; `*T → *mut T` ❌.

### Осознанные trade-off'ы
1. **Deep-immutable сквозь `*mut` нельзя навязать снаружи** (C++ shallow-const): `-> ro VR` морозит свои слоты, но `unsafe{*v.p=w}` проходит (L2 не лезет за `*`). Deep-ro → **производитель** объявляет поле `*T` (как `str { ptr *u8 }`).
2. **Shared-mut heap-record под чужим `ro`** возможен (GC, нет эксклюзивности): `ro` = per-path write-ban, не object-freeze.
3. **owned-vs-aliased heap-record статически неразличим** → граница рисуется **синтаксически на `*`** (L2 стоп на `*`), не по aliasing-статусу.

---

## Нормативный ORACLE (тест-корпус; чтение всегда ✅, знаки = ЗАПИСЬ)

**A. VALUE-record `Point` (копия):** `mut r`: `r=X`✅ `r.x=5`✅ · `ro r`: ❌/❌ · `mut r ro Point`: ✅/❌ · `ro r mut Point`: ❌/✅
**B. HEAP-record `Acc` (handle):** те же знаки (семантика: запись видна co-handle'ам; `ro` = это имя не пишет).
**C. POINTER (unsafe-ops):** `mut p *T`: `p=q`✅ `*p=v`❌ · `mut p *mut T`: ✅/✅ · `ro p *T`: ❌/❌ · `ro p *mut T`: ❌/✅ · `ro p **T`: `p`❌ `*p`❌ `**p`❌ · `ro p *mut *T`: ❌/`*p=q`✅/`**p`❌ · `ro p **mut T`: ❌/`*p`❌/`**p=v`✅
**D. RETURN:** `-> Value`: caller mut-default (`a=X`✅,`a.x=5`✅) · `-> ro Value`: `mut a Value=f()`→`E_READONLY_COERCE`, `mut a ro Value=f()`✅, `ro a mut Value=f()`→`E_READONLY_COERCE`, `ro a Value=f()`✅ · `-> *mut T`: `*a=v`✅(unsafe) · `-> *T`: `*a=v`❌
**E. Generic/Option/cast:** `Vec[*T]`: `*v[i]=x`❌ (L3 элемента), `v[i]=q` через `@MutIndex`+mut-receiver (impl-dependent) · `Vec[*mut T]`: `*v[i]=x`✅ · `Option[*mut T]`: `Some(p)→*p=v`✅ · `x as *mut T; ro a=x`: `a=y`❌ `*a=v`✅ (из типа) · `mut a=x`: ✅/✅ · vr`{p *mut T}`→`ro v`: `v.p=q`❌ `unsafe{*v.p=w}`✅ · `str{ptr *u8,len int}`: `s.ptr=q`❌, буфер ro.

---

## Фазы (атомарные; production; per-phase commit; pos+neg via релизный nova)

### Ф.1 — Откат flip-scan + новый D-block
- `befe92c` (D245 flip-scan spec) — **RETRACT баннер** в 02-types.md (D245 строки ~8500-8644).
- Восстановить **`*T ≡ *ro T` универсально** (02-types.md:7519-7521/7556). Переписать pointer-binding таблицу (7547-7558): убрать flip-scan/current; pointee-mut из ТИПА, reassign из binding; mut-pointee при mut-binding = **явный** `mut p *mut T`.
- **Новый D-block (взамен D245): «Три оси мутабельности»** — P1-P10 + нормативный oracle. Cross-ref D33/D36/D175(§V2 KEEP)/D176/D184/D216-§V2.6(restored).
- 147-doc → этот. Plan 147(flip-scan) SUPERSEDED. README D-index: D245→RETRACTED. **Commit:** `spec(plan147 Ф.1): 3-axis mutability D-block; retract D245 flip-scan; restore *T≡*ro T`.

### Ф.2 — Parser (L1/L2/L3 синтаксис)
- `*ro T` → `E_REDUNDANT_POINTER_RO` (hard error, fix-it «`*T`»). Prefix `mut *`/`ro *` → `E_POINTER_PREFIX_MODIFIER` (выверить). `*T`=ro, `*mut T`=mut, без current/flip.
- L2 `ro T`/`mut T` перед типом (value/record); L1 `ro x`/`mut x` перед именем. AST несёт оси раздельно.
- **Commit:** `feat(plan147 Ф.2): parser 3-axis (no flip-scan; *ro hard error; L1/L2/L3 slots)`.

### Ф.3 — Checker (семантика осей)
- L3: pointee-mut **из типа** (`*T`→ro, `*mut T`→mut), позиционно-независимо. `*ro T` write → `E_POINTER_RO_ASSIGN`.
- L2: транзитивный freeze по owned-графу (access-time, D175 §V2), **СТЕНА на `*`** (P4). Голый `ro x` = freeze (P7). Split `ro x mut T`/`mut x ro T` — явные.
- Coercion: content-widening (ro→mut) → `E_READONLY_COERCE` (по L2, независимо от L1). `*mut→*T` авто-сужение; `*T→*mut` ❌.
- **Commit:** `feat(plan147 Ф.3): checker L2-freeze (wall at *), L3-from-type, content-coercion`.

### Ф.4 — Миграция codebase
- `*ro T` → `*T` (теперь error) — все декларации. **str-поле `ptr *ro u8` → `ptr *u8`** (02-types.md:7525, 08-runtime.md:745, D26).
- `mut data *T` (flip-scan-канон mut-pointee) → **`mut data *mut T`** (restored-канон) — vec_owned.nv:97/750 + все, где нужен mut-pointee.
- Все pointer/value-декларации к 3-axis-канону. Broad-регрессия 0 new FAIL. **Commit(s):** per-кластер.

### Ф.5 — Тесты (oracle-корпус)
- `nova_tests/plan147/`: pos+neg по ВСЕЙ oracle-таблице (A-E, ~20 форм): value/heap split, pointer-уровни, return-coercion (4 случая), `*ro`→error, prefix→error, `*v[i]` vs `v[i]`. **Commit:** `test(plan147 Ф.5): 3-axis oracle pos/neg corpus`.

### Ф.6 — Docs + close + gate-release
- project-creation + simplifications + backlog (закрыть `[M-138-binding-type-mut-conflict]` — разрешён P6; `[M-ptr-cast-reinterpret-unsafe]` — учесть в coercion; убрать flip-scan-маркеры) + nova-private/discussion-log + memory. Push.
- **Снять гейт** Plan 139 `[M-139-f0-lang-item-decl]`: str-поля = `ptr *u8`, чистая модель, flip-scan не нужен.

---

## Acceptance
- **A1** — oracle-таблица (A-E, ~20 форм): pos компилируются, neg дают точные ошибки (`E_REDUNDANT_POINTER_RO`, `E_POINTER_PREFIX_MODIFIER`, `E_READONLY_COERCE`, `E_POINTER_RO_ASSIGN`). plan147 GREEN.
- **A2** — `*T ≡ *ro T` ВЕЗДЕ (param/return/generic/alias/cast/field/local) — самодостаточность типа. Доказано позиционными фикстурами.
- **A3** — L2 freeze транзитивен по owned-графу + СТЕНА на `*` (P4/P9: vr с `*mut`-полем — `v.p` frozen, `*v.p` writable). 
- **A4** — split `ro r mut Point`/`mut r ro Point` ✅; голый `ro r` = freeze. Return-coercion 4 случая.
- **A5** — D245 retracted; pointer-таблица + str переписаны; D-block «3 оси» + oracle нормативны.
- **A6** — 0 регрессий vs baseline; pointer-using dirs (plan115/118/138/128) clean.

## Risks
| # | Риск | Sev | Mitigation |
|---|---|---|---|
| R1 | Откат свежей flip-scan-спеки + миграция многих сайтов | 🔴 HIGH | Ф.4 broad-скан + регрессия per-кластер |
| R2 | L2 access-time freeze (transitive, wall-at-*) тонко в checker | 🔴 HIGH | oracle как оракул; D175 §V2 уже реализует L2 (переиспользовать) |
| R3 | `mut data *T`→`*mut T` миграция Vec может задеть codegen | 🟡 MED | plan138_* регрессия; 138.4 G-D pointee preservation |

## Связь
- **Отменяет** D245 (flip-scan) + восстанавливает D216 §V2.6 (`*T≡*ro T`).
- **D175 §V2 (binding-dominates/access-time)** = L2, не трогаем (+ «стоп на `*`»).
- Разрешает `[M-138-binding-type-mut-conflict]` (P6 split на L1×L2).
- Гейтит **Plan 139 `[M-139-f0-lang-item-decl]`** (str `ptr *u8` под 3-axis).
- Источник: 2 design-workflow (critique wkx3dytr1, value-side wlqgc2nyk, synthesis w9nktq8x1) + ~15 раундов ревью.
