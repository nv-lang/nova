<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# Plan 191 — bcrypt: закрыть security-surface (демо-KDF под именем bcrypt)

**Статус:** ✅ **Вариант A ЗАКРЫТ 2026-07-10** (ветка `bcrypt-derisk-191`, коммит
`0f72ed25e`). Вариант B (настоящая KDF — Argon2/native Blowfish-шим) —
**Plan 193**, отдельная будущая волна. **Приоритет:** P1-surface (латентная).
**Маркер:** `[M-bcrypt-demo-kdf-not-real]` — A закрыт, B=Plan 193.

### Итог варианта A (2026-07-10)

- `git mv std/crypto/bcrypt.nv` → `std/_experimental/crypto/insecure_demo_kdf.nv`
  (+ peer-тест) — auto-skip из `nova check std/`, вне shipped 0.1 набора.
- `Bcrypt`/`BcryptError` → `InsecureDemoKdf`/`InsecureDemoKdfError`.
- Формат `$2b$<cost>$...` → собственный `$ndk1$<cost>$...` — `verify`
  намеренно НЕ распознаёт `$2b$`/`$2a$`/`$2y$` (пин-тест на отказ настоящих
  bcrypt-хешей, чтобы не создавать иллюзию совместимости).
- Докстринг — явное `⚠ INSECURE` в шапке + на каждой public fn
  (`#experimental` вместо `#stable`).
- Сняты живые упоминания `Bcrypt` как shipped-примера (`std/nova.toml`,
  `std/_experimental/STATUS.md`, `spec/decisions/07-modules.md`,
  `std/prelude/effects.nv`, `std/testing/handlers.nv`).
- Гейты: cargo build OK; conformance 91/0; `nova check std/` FAIL-set δ0
  (32 pre-existing, 0 crypto; sha256/hmac/md5/sha1/jwt не задеты);
  `insecure_demo_kdf` check+test --full PASS (8 тестов, вкл. новый
  negative на отказ `$2b$`); grep-инвариант 0 `Bcrypt`/`$2b$` в shipped std/.

## Проблема — да, это латентная дыра безопасности

`std/crypto/bcrypt.nv` экспортирует `Bcrypt.hash(password, cost)` /
`Bcrypt.verify`, но **внутренность — НЕ настоящий bcrypt**: вместо
Blowfish/EksBlowfishSetup — упрощённый SHA-256 key-stretching (шапка файла честно
пишет «демонстрация API, не production-grade»).

**Почему это риск:** предупреждение живёт в комментарии .nv-исходника — пользователь,
делающий `import std.crypto.bcrypt` и вызывающий `Bcrypt.hash`, его НЕ видит. Хеш
называется bcrypt-форматом (`$2b$...`), выглядит как настоящий → человек хранит
пароли, считая их защищёнными bcrypt'ом, а на деле — быстрым SHA-стретчингом
(GPU-attackable). Это классический «footgun по умолчанию».

## Две дороги

### A. ДЕШЁВЫЙ де-риск (рекомендуется СЕЙЧАС, ~1 малая волна)

Не даём shipped-std API называться `Bcrypt` пока он не настоящий bcrypt. Варианты
(sign-off владельца — какой):
1. **Убрать из shipped std** → `std/_experimental/crypto/bcrypt.nv` (не в MVP-наборе,
   импорт явно «экспериментальное»).
2. **Переименовать в честное имя** — `InsecureDemoKdf` / `Pbkdf2Sha256Demo`, `$2b$`
   формат снять (не выдавать за bcrypt), докстринг с `#deprecated`/`#unsafe`-маркером.
3. **Runtime-гейт** — требовать явный `Bcrypt.hash_insecure_demo(...)` или флаг
   `allow_insecure`, иначе `Err`/compile-warn.
Стоимость: низкая (переименование/перенос + правка тестов). Закрывает
восприятие-риск немедленно, НЕ требует крипто-инженерии.

### B. НАСТОЯЩАЯ реализация (дорого, отдельная будущая волна)

Полный Blowfish + EksBlowfishSetup (~600 LOC) с верификацией против тест-векторов,
ЛИБО argon2 (OWASP-рекомендация 2026), ЛИБО binding к libsodium/native-шиму
(как tls_shim — Rust-крейт `bcrypt`/`argon2` через C-ABI). Требует тщательной
крипто-верификации (константное время, тест-векторы RFC/OpenBSD). НЕ дёшево, НЕ
в хвосте волны.

## Рекомендация

Сделать **A сейчас** (дёшево, закрывает опасное восприятие), **B** — запланировать
отдельно (возможно через native-шим по образцу Plan 192). Владелец выбирает вариант A.

## Гейты

Для A: `Bcrypt`-имя/формат больше не выдаёт себя за настоящий bcrypt в shipped std;
существующие тесты мигрированы на новое имя/расположение; conformance δ0.

## Границы

Не трогает другие crypto-модули (sha/hmac — настоящие, NIST-верифицированы).
