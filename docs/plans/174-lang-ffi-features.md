# Plan 174 — Language & FFI features on the unified type engine (umbrella)

**Status:** 📋 proposed (umbrella, 2026-06-27)
**Carrier dependency:** **Plan 172.1** (unified type engine). Все под-планы 174.x «садятся на»
единый движок типов 172.1 (типизированный IR, lossless `ResolvedType`, единый реестр) — они
**зависят от 172.1, но не блокируют его** и являются отдельными deliverable'ами.

## 0. Происхождение

Изначально эти фичи были самостоятельными планами **171 / 174 / 175 / 176 / 177 / 178**. Аудит
2026-06-19 свернул их в зонт 172 (как 172.6-172.11, коммит `69d3e5e5`), т.к. все они опираются на
носитель 172.1. Решением владельца (2026-06-27) они **вынесены обратно в отдельный зонт 174**: зонт
172 остаётся сфокусированным на ядре движка типов (**172.1-172.5**), а 174 группирует независимые
языковые/FFI-фичи поверх этого носителя. Forward-compat-связь «172.1 не должен форклоузить 174.x»
описана в [172-compiler-rework.md](172-compiler-rework.md) §3.1/§3.2.

## 1. Под-планы

| # | План | Суть | Статус |
|---|---|---|---|
| **174.1** | [Primitive parse API](174.1-primitive-parse-api.md) | Один движок str→примитив, radix-only `parse`; per-type обёртки с range-check; фикс truncation-бага. Зависит 172.3 (type-set bounds схлопывает обёртки). | 📋 proposed |
| **174.2** | [`?` return-only](174.2-question-mark-return-only.md) | `?` строго return-only (Rust-стиль), в Fail-fn запрещён (`!!`/`throw`); чистка stale `## D4`. Завершает 173. | 📋 PLANNED |
| **174.3** | [`any` + downcast](174.3-any-type-and-is-downcast.md) | `any` top-type (fat-pointer) + `is T`/`try_as[T]` runtime-downcast по `type_id`. Разблокирует typed-errors 173 Ф.4. | 📋 PROPOSED (P1) |
| **174.4** | [Effect-registry size](174.4-effect-registry-compile-time-size.md) | Compile-time размер effect-registry вместо хардкода 32 (>32 эффектов → silent-drop). | 📋 READY (P2) |
| **174.5** | [Pointer-ops methods](174.5-pointer-ops-methods.md) | Операции указателей через методы (`.read`/`.write`/`.offset`/…) вместо операторов; `unsafe T`→`uninit T`; write-cap fix. | 📋 PROPOSED |
| **174.6** | [C-FFI ABI types](174.6-ffi-abi-types.md) | C-ABI тип-лист (туплы/value-records/`Option[*T]` рекурсивно) + fn-ptr ABI-тег (`*extern "C" fn` vs `*fn`). | 📋 PROPOSED |

## 2. Зависимости и порядок

- **Все 174.x зависят от носителя 172.1** (единый движок типов). До landing MVP 172.1 (U.1-U.4)
  они идут как дизайн/spec; реализация — на готовом носителе.
- **174.1** (parse-api) дополнительно зависит от **172.3** (type-set bounds схлопывает ~13 per-type
  обёрток в ~2-3 generic).
- **174.2/174.3** связаны с **Plan 173** (error-system): `?`-return-only завершает 173; `any`+downcast
  разблокирует typed-errors 173 Ф.4.
- Между собой 174.x **независимы** (разные подсистемы: parse / error-syntax / type-system / effects /
  pointers / FFI) — могут вестись параллельно.

## 3. Закрытие

Зонт 174 закрыт, когда landed все 174.1-174.6 (каждый со своими acceptance-критериями в своём
файле). Носитель — 172.1; 174 не входит в критерии закрытия зонта 172.
