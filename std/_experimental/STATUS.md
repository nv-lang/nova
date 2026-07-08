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

## Содержимое (Plan 91 Ф.0 baseline 2026-05-27; ред. волны промоушена 2026-07-08)

> **PROMOTED 2026-07-08 (волна 1, 13 модулей):** `checksums/{crc32,fnv}` →
> `std/checksums/`; `collections/{bloom_filter,deque,lru,priority_queue,queue}` →
> `std/collections/`; `concurrency/rate_limiter` → `std/concurrency/`;
> `crypto/bcrypt` → `std/crypto/`; `identifiers/snowflake` → `std/identifiers/`;
> `math/statistics` → `std/math/`; `path/{glob,path}` → `std/path/`.
> Каждый — с коррекцией к конвенциям (throw→Result D325, без `_opt`, канон
> `.new().cap(n)`, снос битых конструкторов) и peer-тестами `*_test.nv` рядом.
> `math/complex` планировался в волну, но остаётся здесь — блокирован
> pre-existing codegen-дефектом `[M-static-selfreturn-value-mangle-conflict]`
> (см. docs/plans/backlog-followups.md).

> **PROMOTED 2026-07-08 (волна 2, 14 модулей):** `crypto/{sha256,hmac,md5,
> sha1,jwt}` → `std/crypto/`; `encoding/{hex,ini}` → `std/encoding/`;
> `identifiers/{ulid,uuid}` → `std/identifiers/`; `time/cron` → `std/time/`;
> `text/{diff,markdown_minimal,regex}` → `std/text/`; `data/{semver,sql}` →
> `std/data/` (новые каталоги). Каждый — с peer-тестами `*_test.nv`,
> explicit imports вместо implicit cross-module resolution (последнее
> проходит `nova check`, но ломает codegen — P67-LEGACY/mono-erasure),
> error-path тесты на flag-idiom (`|e| { caught = e is Variant; fallback }`
> вместо `interrupt Some(e)` + `match Some(Variant{..})` — второе даёт
> CC-FAIL на struct-payload enum). Найдены и исправлены 3 pre-existing
> content-бага (не promotion-механика): markdown_minimal literal-text не
> экранировал HTML; semver.from молча принимал trailing `-`/`+` без
> identifier; sql.nv не импортировал `std.text` для `[]str.join()`.
> `csv`/`toml`/`url` (encoding), `uuid_namespace` (identifiers),
> `linkedlist` (collections), `retry` (concurrency) остаются здесь —
> каждый PASS `nova check`, но CC-FAIL/CODEGEN-FAIL под `nova test --full`
> на РАЗНЫХ genuine compiler-дефектах (см. таблицу и
> docs/plans/backlog-followups.md для деталей каждого).

| Domain | Files | Status | Reason for exp. |
|---|---|---|---|
| `collections/` | `linkedlist` (остальные 5 PROMOTED 2026-07-08 w1) | check PASS, test--full CC-FAIL (2×) — self-recursive generic `.map()` P67-LEGACY + cross-CU `LinkedList[T].new()` mono-loss | Промоушен gated compiler-дефектом (recursive generic sum-type mono) |
| `crypto/` | (пусто — все 5 PROMOTED 2026-07-08 w2: sha256/hmac/md5/sha1/jwt) | — | — |
| `encoding/` | `csv`, `toml`, `url` (`hex`/`ini` PROMOTED 2026-07-08 w2) | check PASS, test--full CC-FAIL/CODEGEN-FAIL (3 разных дефекта — см. inline-комментарии в файлах) | Промоушен gated compiler-дефектами: csv — nested `[][]str` aliasing runtime-баг; toml — Fail-handler mono gap (`Nova_*Error_p` unknown type); url — документированный tuple-destructure infer баг (Plan 06 Ф.2), подтверждён под codegen |
| `identifiers/` | `uuid_namespace` (`ulid`/`uuid` PROMOTED 2026-07-08 w2, `snowflake` w1) | check PASS, test--full CC-FAIL — duplicate C-symbol emission (`crypto.md5.rotl32` эмиттится дважды когда `md5`+`sha1` в одном CU) | Промоушен gated general compiler-дефектом (private-fn dedup emission), репродуцирован минимальным файлом |
| `data/` | `semver_range` | PASS check (не проверялся в волне 2 — не в списке переноса) | Non-MVP per Plan 91 §Non-scope; `semver`/`sql` PROMOTED 2026-07-08 w2 |
| `math/` | `complex` (`statistics` PROMOTED 2026-07-08 w1) | check PASS, CC-FAIL codegen — `[M-static-selfreturn-value-mangle-conflict]` | Промоушен gated компиляторным дефектом (не контентом модуля) |
| `text/` | (пусто — все 3 PROMOTED 2026-07-08 w2: diff/markdown_minimal/regex) | — | — |
| `time/` | (пусто — cron PROMOTED 2026-07-08 w2) | — | — |
| `concurrency/` | `retry` (`rate_limiter` PROMOTED 2026-07-08 w1) | check PASS (текущая `[T]`-сигнатура), test--full CC-FAIL (E инферится per-call-site, конфликт между T-инстанциями); ПРАВИЛЬНАЯ `[T, E]`-сигнатура (matching doc-comment) REJECTED checker'ом D145 (generic E used только в effect-clause не считается "used") | Two-sided блокировка — implicit-E даёт codegen-баг, explicit-E упирается в checker-ограничение; нужен архитектурный выбор, не в рамках промоушен-волны |

Модули выше с CC-FAIL/CODEGEN-FAIL под `test --full` **сохраняют** свои
peer inline-тесты (не удалены) с комментарием `[2026-07-08, волна 2
промоушена]`, объясняющим конкретный дефект — готовы к промоушену как
только соответствующий codegen-баг будет исправлен (см.
docs/plans/backlog-followups.md).

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
