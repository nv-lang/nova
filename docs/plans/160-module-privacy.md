<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# Plan 160 — Module-level field privacy (`type X priv { … }`)

> **Создан:** 2026-06-15. **Статус:** ✅ ЗАКРЫТ (Ф.1–Ф.3 выполнены, 2026-06-15).
> **Владеет:** `[M-160-module-privacy]`. **D-блок:** D281.
> **Зависит от:** checker (visibility), codegen (нет изменений).

## Проблема

Nova's папочная модель модулей (папка = один модуль, несколько co-equal `.nv` файлов)
предполагает, что связанные типы живут рядом. Например:

```
scheduler/
  job.nv    ← тип Job
  queue.nv  ← читает поля Job
  worker.nv ← читает поля Job
```

До Plan 160 существовало только два уровня видимости поля:
- **public** (нет модификатора) — поля видны всем.
- **`priv` field-level** — поля видны только внутри методов самого типа.

Нет промежуточного уровня: «поля доступны внутри модуля (папки), но не снаружи».
Если `Job` нужен `queue.nv` и `worker.nv` — выбор: либо все поля публичны (утечка
инкапсуляции), либо каждый доступ оборачивается в метод-акцессор (бойлерплейт).

## Решение (D281)

Два уровня type-level privacy modifier:

```nova
// module-private default (D281 — новое):
type Job value priv {
    mut id   int      // module-private: виден в том же модуле, не снаружи
    kind     int      // module-private
    priv secret int   // type-private: только методы Job
}

// type-private default (D220 amend — усилен):
type Secret priv(type) {
    key u64           // type-private: только методы Secret
    pub tag str       // override → public
}
```

### Финальный дизайн синтаксиса

| Type-level modifier | Семантика полей по умолчанию |
|---|---|
| (нет) | public (D47) |
| `priv` | **module-private** (D281) |
| `priv(type)` | type-private (D220 amend) |
| `priv(module)` | **ОШИБКА** `E_PRIV_QUALIFIER` — использовать `priv` |

Field-level `priv` modifier всегда = type-private, независимо от type-level default.

### Точные правила видимости

| Поле в `type T priv { ... }` | Видно в методах T | Видно в том же модуле | Видно снаружи |
|---|---|---|---|
| `field T` (без модификатора) | ✅ | ✅ | ❌ `E_FIELD_MODULE_PRIVATE` |
| `priv field T` | ✅ | ❌ `E_PRIV_FIELD_READ` | ❌ `E_PRIV_FIELD_READ` |

Сам тип `T` при этом остаётся публичным — `priv` на уровне типа не ограничивает
видимость **типа**, только видимость **полей** по умолчанию.

## Фазы

### Ф.1 — Парсер: `priv` / `priv(type)` на объявлении типа ✅

- `FieldDefaultVisibility` enum: `Public` / `Module` / `Private`.
- Bare `priv` после type-modifiers → `Module`.
- `priv(type)` → `Private`.
- `priv(module)` → hard error `E_PRIV_QUALIFIER`.
- `RecordField.priv_module_field: bool` + `NamedTupleField.priv_module_field: bool`.

### Ф.2 — Checker: enforcement field-access ✅

- `TypeCheckCtx.type_defining_modules: HashMap<String, Vec<String>>` — строится из `peer_files.items_here`.
- `TypeCheckCtx.current_module: RefCell<Vec<String>>` + `CurrentModuleGuard` RAII.
- `module_priv_access_allowed(tname)` — сравнивает home-module type'а с current_module.
- 5 check-сайтов: INIT, READ (Record + NamedTuple), WRITE, PATTERN.
- `priv_module_field=true` → `E_FIELD_MODULE_PRIVATE`; `priv_field=true` → D220 codes.

### Ф.3 — Тесты и spec (D281) ✅

Тесты: `nova_tests/plan160/` — **5/5 PASS**.

**Позитивные:**
- `pos_within_module.nv` — 4 теста: read, write, method, constructor в том же модуле.

**Негативные:**
- `neg_read_outside.nv` — `E_FIELD_MODULE_PRIVATE` при чтении поля из другого модуля.
- `neg_write_outside.nv` — `E_FIELD_MODULE_PRIVATE` при записи поля из другого модуля.
- `neg_init_outside.nv` — `E_FIELD_MODULE_PRIVATE` при init-литерале из другого модуля.
- `neg_priv_field_same_mod.nv` — `E_PRIV_FIELD_READ` для `priv` поля в `priv`-типе из свободной функции.

**Spec:** D281 в `spec/decisions/02-types.md` — полный блок с §1–§5.

## Критерии приёмки (без упрощений, как для прода)

- **A1.** `type T priv { f int }` — поля без модификатора доступны в том же модуле без ошибок.
- **A2.** Доступ из другого модуля → `E_FIELD_MODULE_PRIVATE` (не crash, не silent).
- **A3.** `priv` field в `priv`-типе → `E_PRIV_FIELD_READ` даже из того же модуля (из свободной функции).
- **A4.** Все 5 fixtures PASS (4 позитивных теста + 4 негативных = 5 test-файлов).
- **A5.** Нет регрессий в `nova test` core-suite.
- **G0.** fail safe = запретить при неопределимой видимости.

**Статус:** все критерии выполнены ✅

## Отложено / out of scope

- Per-field `priv(module)` аннотация (не нужна для целевого use case — type-level достаточно).
- `priv` / `pub(module)` на методах (методы не имеют module-level granularity — separate task).
- Named tuple `priv` (D225 — отдельный plan).
