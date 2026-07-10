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
| `encoding/` | `toml` (`hex`/`ini` PROMOTED w2, `url` PROMOTED batch 2, **`csv` PROMOTED 2026-07-08 batch 3**) | toml: check PASS, test--full CC-FAIL — the original Fail-handler mono gap (`Nova_*Error_p` unknown type) is FIXED (batch 3, `debt_unmangle_ptr_suffix`), but a NEW, deeper defect now surfaces: `Nova_HashMap____nova_str__Nova_TomlValue_p` unknown-type (forward-declare/hoist ordering for a mono struct used as a sum-variant's payload field — typedef exists later in the same file) | toml: gated by a hoist-ordering defect that looks like the same zone reserved for [M-option-self-recursive-record-mono] — re-check after that lands |
| `identifiers/` | (пусто — `snowflake` PROMOTED w1, `ulid`/`uuid` PROMOTED w2, **`uuid_namespace` PROMOTED 2026-07-08 batch 3**) | — | — |
| `data/` | (пусто — `semver`/`sql` PROMOTED 2026-07-08 w2, **`semver_range` PROMOTED 2026-07-10**) | — | — |
| `math/` | `complex` (`statistics` PROMOTED 2026-07-08 w1) | check PASS, CC-FAIL codegen — `[M-static-selfreturn-value-mangle-conflict]` | Промоушен gated компиляторным дефектом (не контентом модуля) |
| `text/` | (пусто — все 3 PROMOTED 2026-07-08 w2: diff/markdown_minimal/regex) | — | — |
| `time/` | (пусто — cron PROMOTED 2026-07-08 w2) | — | — |
| `concurrency/` | (пусто — `rate_limiter` PROMOTED w1, `retry` PROMOTED 2026-07-08 batch 2) | — | — |

> **PROMOTED 2026-07-08 (batch 2, Plan 172.13, 2 modules):** `concurrency/
> retry` → `std/concurrency/`; `encoding/url` → `std/encoding/`. Both
> unblocked by genuine compiler-channel fixes (not workarounds), with
> peer-tests split to `retry_test.nv`/`url_test.nv`:
>   - retry: D145's `E_UNUSED_PREFIX_TYPEVAR` never scanned a fn-typed
>     param's own effect-clause (`body fn() Fail[E] -> T`) nor the method's
>     own effect-clause when deciding if a prefix typevar is "used" — fixed
>     in the checker (types/mod.rs). Once accepted, the SAME missing
>     "infer E from a closure's thrown value" source was needed in THREE
>     independent duplicated codegen engines, plus a downstream `Ok`/`Err`-
>     inside-`with Fail[E]`-block hint-threading gap, plus a handler-literal
>     forward-decl gap for multi-instantiation generic methods (see the
>     batch-2 commit series for the full chain).
>   - url: the tuple-shape mono conflict (match-arm reconciliation only
>     recognized bare `nova_int` as an ambiguous default, not a tuple
>     composed entirely of it) and the sibling-Option case of the same
>     `Fail[E]`-hint gap (extended from Result to Option). One content bug
>     (`parse_authority`'s empty-authority branch returned `host: None`
>     instead of `Some("")` for `file:///etc/hosts`).
> `csv`/`toml` (encoding), `uuid_namespace` (identifiers) remain — each now
> blocked by a DIFFERENT, narrower defect than originally diagnosed (see
> the table below and docs/plans/backlog-followups.md
> `[M-exp-promotion-blockers]`).

> **PROMOTED 2026-07-10 (1 module):** `data/semver_range` → `std/data/`
> (peer к `semver`, PROMOTED w2). Коррекции при промоушене: import
> `semver` → `std.data.semver`; методы сравнения Version под актуальный
> API (`@equal` + операторы `< > <= >=` через `@compare` вместо
> ретрактированных `eq/gt/ge/lt/le`); `parse_version` — честный `match`
> по `Result` из `to_version()` (старый cross-effect `with`-хендлер был
> под Fail-версию semver-парсера + генерил CC-FAIL: лямбда-хендлер не
> захватывал `s`); в тестах `to_version()!!` (Result, не Version).
> Inline-тесты вынесены в peer `semver_range_test.nv`
> (module data.semver_range_test); check PASS + test --full зелёный.

> **PROMOTED 2026-07-08 (batch 3, Plan 172.13, 2 modules):** `encoding/csv`
> → `std/encoding/` (unblocked by [M-consume-rebind-nested-block-shadow]
> batch-3 fix — consume-rebind in a nested if/else now reuses the enclosing
> C variable); `identifiers/uuid_namespace` → `std/identifiers/` (unblocked
> by [M-random-u64-path-return-ice] — Random effect declaration moved to
> the prelude — plus a batch-3 follow-up to the batch-2 dedup: the
> qualified mangled name of a colliding cross-module private fn now also
> reaches the FORWARD declaration via `mangle_fn`, closing the
> "conflicting types for `nova_fn_6crypto3md56rotl32`" implicit-declaration
> CC-FAIL). Both with peer `*_test.nv` split per the w1/w2 convention;
> check + test --full green. `toml` stays (hoist-ordering defect, see
> table); `linkedlist` stays (owner-agent zone).

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
