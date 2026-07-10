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

> **PROMOTED 2026-07-10 (Plan 186 recursive-mono, 1 модуль):**
> `collections/linkedlist` → `std/collections/`. Разблокирован ПОЧИНКОЙ
> самого mono-канала (не воркэраундом контента) — четыре сцепленных
> compiler-дефекта в recursive-generic-sum-type мономорфизации:
> (1) [M-result-direct-recursive-enum] / [M-option-self-recursive-record-mono]
> (emit_c.rs `emit_field_eq`/`register_novaopt_decl`/`being_defined_record_types`
> — экспоненциальный разворот структурного `==` и mistype self-recursive
> `Option[Self]`-поля, см. docs/plans/backlog-followups.md);
> (2) [M-generic-method-self-recursive-return] — P67-LEGACY паника на
> самовызове `t.map(f)` внутри `@map[U]`'s собственного тела (return-тип не
> резолвился ни checker'ом, ни legacy fn_ret-реестром для mono'd instance
> самого себя); той же волной — cannot-infer method-level `U` на этом же
> самовызове (`resolve_method_level_subst` не имела Source 2f для
> method-level typevar'ов, только free-fn версия её имела);
> (3) `Nova_`-префикс naming bug в operator-overload dispatch (`@plus`
> генерил вызов `Nova_LinkedList____nova_int_method_plus`, а mono-эмиттер
> называет метод БЕЗ префикса) и в bare-variant ctor dispatch (`Empty`
> внутри `.new() -> Self => Empty` резолвился в `nova_make_LinkedList____
> nova_int_Empty` вместо реально эмитированного `nova_make_Nova_
> LinkedList____nova_int_Empty`) — обе ветки чинились по одному паттерну:
> детект mono-имени по разделителю `____`, разный naming-convention для
> plain-sum vs generic-mono-instance.
> Коррекции к конвенциям при промоушене: `@length()` → `@len()`,
> `@reverse()` → `@reversed()` (обе коллидировали с method-name single-key
> fallback реестром, который last-wins разрешает ambiguous dispatch —
> `len`/`reverse` УЖЕ заняты Vec/Range/HashMap; после 3-го регистранта
> резолюция ломалась для ВСЕХ трёх — pre-existing систем­ная хрупкость
> метод-диспетчера, вне scope этого плана, обойдена переименованием);
> `from_iter(it Iter[T])` → `from_iter(items []T)` (зеркало
> [M-91.1-set-from-iter-iterable-param] — `Iter[T]` стирается в `void*` в
> mono'd generic-теле, `for-in` не мог восстановить C-тип итератора);
> bare unqualified `Empty` в тестах заменён на `.new()` (bare-вариант
> неоднозначен, когда ДРУГОЙ sum-тип В ТОЙ ЖЕ compilation unit ТОЖЕ
> объявляет вариант `Empty` — например prelude-ошибка парсинга; резолюция
> unqualified-формы не гарантированно учитывает даже explicit type
> annotation — известное pre-existing ограничение, не тронуто).
> Инлайн-тесты вынесены в peer `linkedlist_test.nv`; doc-примеры `let` →
> `ro` (D184); D406 enum-маркер добавлен в header-комментарий примера.
> check PASS + `test --full` зелёный.

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
| `collections/` | (пусто — `linkedlist` **PROMOTED 2026-07-10** Plan 186; остальные 5 PROMOTED 2026-07-08 w1) | — | — |
| `crypto/` | (пусто — все 5 PROMOTED 2026-07-08 w2: sha256/hmac/md5/sha1/jwt) | — | — |
| `encoding/` | (пусто — `toml` **PROMOTED 2026-07-10** [M-toml-repeated-fail-call-run-fail] fix; `hex`/`ini` PROMOTED w2, `url` PROMOTED batch 2, `csv` PROMOTED 2026-07-08 batch 3) | — | — |
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

> **PROMOTED 2026-07-10 (1 module): `encoding/toml` → `std/encoding/`.**
> Closes `[M-toml-repeated-fail-call-run-fail]` — investigated end to end;
> the marker's original hypothesis (a Fail-effect/fail-frame/consume-scope
> reentrancy bug on repeated same-scope calls) was WRONG. Root cause was
> TWO unrelated, purely-local bugs in toml.nv itself:
>   1. `is_bare_key_char`'s multi-line `||`-chain used a LEADING `||` on
>      each continuation line. `||` is ALSO the zero-arg closure-literal
>      syntax (`|| body`) — the parser (`parse_or`,
>      compiler-codegen/src/parser/mod.rs) deliberately does not extend
>      newline-tolerance to a line-initial `||` (to avoid misparsing a
>      genuine `|| body` closure statement as an OR-continuation). Each
>      leading-`||` line silently became its OWN discarded zero-arg
>      closure-literal statement; the function's trailing value ended up
>      being the LAST closure's pointer coerced to `nova_bool` — always
>      truthy, regardless of the input character. Neither `nova check` nor
>      codegen flagged this (no diagnostic) — tracked separately as a
>      checker-hardening follow-up (closure-typed trailing expr vs a scalar
>      return type should be a type error). Fix: move `||` to the END of
>      each line (trailing operator before newline IS continuation, no
>      closure-literal ambiguity there).
>   2. `@parse_number` called the RETRACTED `f64.try_from`/`i64.try_from(str)`
>      surface ([M-f64-try-parse-to-parse-f64], Plan 174.1 — known-broken,
>      e.g. `f64.try_from("3.14")` silently returns `3.0`). Fix: canon
>      conversion-on-source `str @to_f64()`/`str @to_i64()`
>      (std/runtime/string/parse.nv).
> Repro method: minimal standalone `is_bare_key_char`-shaped fn (no toml,
> no Fail, no consume) reproduced bug 1 in isolation; a direct
> `f64.try_from("3.14")` call reproduced bug 2 in isolation — both
> confirm neither bug has anything to do with Fail-effect repetition. New
> positive regression tests pin the ACTUAL (unbroken) repeated-Fail-call-
> in-one-with-scope mechanism (`std/encoding/toml_test.nv`). Peer test
> split: `toml.nv` (impl) / `toml_test.nv` (public-contract tests, mirrors
> csv/hex/ini/url convention). Model: sonnet.

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
