# std/_experimental/ — non-MVP stdlib modules

**Created:** 2026-05-27 (Plan 91 Ф.7.1 quarantine).
**Status:** EXPERIMENTAL — не входит в 0.1 release contract.

## Зачем underscore-prefix

Каталог `_experimental/` начинается с `_` — это auto-skip триггер в
`nova check` (см. `should_skip_path_full` в nova-cli/src/main.rs:1482).
Файлы здесь:

- **НЕ** проверяются через `nova check std/` (skip по path component);
- **НЕ** входят в `nova check` shipping gate для 0.1 release;
- **МОГУТ** быть explicit'но импортированы как
  `import std._experimental.<domain>.<file>` для тестов и
  экспериментального кода;
- **МОГУТ** проходить или НЕ проходить per-file `nova check` (FAIL
  модули документированы ниже).

## Содержимое (Plan 91 Ф.0 baseline 2026-05-27)

| Domain | Files | Status | Reason for exp. |
|---|---|---|---|
| `collections/` | `bloom_filter`, `deque`, `linkedlist`, `lru`, `priority_queue`, `queue` | PASS check | Non-MVP per Plan 91 §Scope (MVP = vec/hashmap/set only) |
| `crypto/` | `bcrypt`, `hmac`, `jwt`, `md5`, `sha1`, `sha256` | 4 FAIL (array literal parser), 2 PASS | Non-MVP per Plan 91 §Non-scope; 4 files trip array literal parser bug |
| `encoding/` | `csv`, `hex`, `ini`, `toml`, `url` | 4 PASS, 1 STACK_OVERFLOW (toml) | Non-MVP per Plan 91 §Non-scope; toml stack overflow blocks `nova check std/` без skip |
| `identifiers/` | `snowflake`, `ulid`, `uuid`, `uuid_namespace` | PASS check | Non-MVP per Plan 91 §Non-scope |
| `checksums/` | `crc32`, `fnv` | PASS check | Non-MVP per Plan 91 §Non-scope |
| `data/` | `semver`, `semver_range`, `sql` | PASS check | Non-MVP per Plan 91 §Non-scope |
| `path/` | `glob`, `path` | PASS check | Plan 18 → 0.2+ (filesystem); non-MVP per Plan 91 §Non-scope |
| `math/` | `complex`, `statistics` | 1 FAIL (complex D52 §2), 1 PASS | Non-MVP per Plan 91 §Scope (MVP math = runtime/math.nv basic fns) |
| `text/` | `diff`, `markdown_minimal`, `regex` | 1 FAIL (regex D52 §2), 2 PASS | Non-MVP per Plan 91 §Non-scope (regex/markdown), text MVP = runtime/string.nv |
| `time/` | `cron` | FAIL (D52 §2) | Non-MVP per Plan 91 §Scope (MVP time = duration only) |
| `concurrency/` | `rate_limiter`, `retry` | 1 FAIL (retry E_UNUSED_PREFIX_TYPEVAR), 1 PASS | Non-MVP — `cancellation`/`timer` остаются в std/concurrency/ как Plan 83 fiber-api |

## Promotion path (когда модуль становится MVP)

Когда модуль готов к shipping в `0.X` (после fix codegen/runtime блокеров):

1. `git mv std/_experimental/<domain>/<file>.nv std/<domain>/<file>.nv`
2. Update импорты в тестах: `std._experimental.<domain>.<file>` → `std.<domain>.<file>`
3. Update `std/STATUS.md` и `std/nova.toml` (этот файл) MVP-набор
4. Verify `nova check std/` → 0 FAIL после move
5. Update `docs/plans/18-stdlib-roadmap.md` — отметить домен как released

## Связь с другими планами

- [docs/plans/91-stdlib-mvp-for-0.1.md](../../docs/plans/91-stdlib-mvp-for-0.1.md) — Plan 91 определяет MVP-набор; Ф.7.1 этот carve-out
- [docs/plans/18-stdlib-roadmap.md](../../docs/plans/18-stdlib-roadmap.md) — полная stdlib roadmap, включая promotion non-MVP в 0.2+
- [docs/plans/14-stdlib-codegen-gaps.md](../../docs/plans/14-stdlib-codegen-gaps.md) — исторический список codegen-блокеров (устарел после Plan 91 Ф.0 re-baseline 2026-05-27)
