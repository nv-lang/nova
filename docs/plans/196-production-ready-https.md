<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# Plan 196 — Production-ready HTTPS (зонт)

**Статус:** 🟡 В РАБОТЕ 2026-07-11 (зонт — сводит разрозненную работу под единые критерии приёмки). **Приоритет:** P1 (запрос владельца «нужен https production-ready»).
**Дом цели «https production-ready»** — раньше размазана по [116](116-std-tls-effect.md)/[178](178-http.md)/[195](195-native-modules-c-not-rust.md); этот план — единая точка.

## Что значит «production-ready https» (критерии приёмки)

HTTPS готов к проду, когда ВСЁ выполнено:
1. **Rust-free бэкенд** — TLS на mbedTLS (C), модуль = `.nv + .c + .lib`, ноль Rust/cargo при сборке модуля (Plan 195).
2. **Полная TLS-поверхность** — TLS 1.2/1.3 handshake, cert-verify (SystemRoots/Insecure/Pinned-SPKI), mTLS (обе стороны), ALPN, SNI, close_notify, стабильная error-классификация.
3. **Нет рантайм-дефектов** — teardown-hang net close-пути починен; cancel-safety (отмена в handshake/read/write не течёт и не виснет); нет segv/hang под стрессом.
4. **Кросс-платформа** — Windows (CLDR/system store) + POSIX; собирается на чистом клоне.
5. **Распространяемый модуль** — вынесен в отдельную репу `nova-tls` (Plan 195 Ф.3, поверх 03.1-резолвера), монорепо тянет как внешнюю зависимость; раскладка `src/`.
6. **Примеры + доки** — examples/tls/echo (клиент+сервер), гайд, D-блоки в спеке.
7. **Промоушен** — std/tls + https-путь в shipped-набор (не _experimental), CI гоняет.

## Сводка под-работ (кто что закрывает)

| Критерий | План/волна | Статус |
|---|---|---|
| 1. mbedTLS Rust-free | [195](195-native-modules-c-not-rust.md) Ф.1-2 (ветка tls-mbedtls-195) | 🔧 В РАБОТЕ [sonnet] |
| 2. TLS-поверхность | [116](116-std-tls-effect.md) Ф.1-5.3 (влито) → пере-пройти на mbedTLS | ✅ на rustls; переносится на mbedTLS |
| 3a. teardown-hang | [M-net-close-teardown-hang] (ветка teardown-hang-close) | 🔧 В РАБОТЕ [sonnet] |
| 3b. cancel-safety | 116 Ф.6 | 📋 не начато |
| 4. кросс-платформа | 116 Ф.6 + mbedTLS system store | 📋 частично (Windows CLDR есть) |
| 5. nova-tls вынос | [195](195-native-modules-c-not-rust.md) Ф.3 + [03.1](03.1-path-git-dependencies.md) | 📋 03.1 ✅ готов; вынос не начат |
| 6. examples+доки | 116 Ф.5.4 + Ф.7 | 📋 не начато |
| 7. промоушен | 116 Ф.7 | 📋 не начато |

## Порядок

1. **mbedTLS-бэкенд** (195 Ф.1-2) — фундамент, всё остальное на нём. [ИДЁТ]
2. **teardown-hang** (параллельно, net-слой). [ИДЁТ]
3. **cancel-safety + кросс-платформа** (116 Ф.6) — после mbedTLS.
4. **examples + доки** (116 Ф.5.4).
5. **nova-tls вынос** (195 Ф.3) — после mbedTLS (чтобы выносить уже Rust-free) + src/-раскладка.
6. **промоушен + CI** (116 Ф.7) — финал.

## Гейты (production-ready)

conformance δ0; std/tls 6/6 на mbedTLS; https-разгейт зелёный; стресс handshake+echo ×30 = 0 hang/segv; cancel-тесты зелёные; grep 0-Rust-в-TLS; nova-tls собирается standalone (только clang+.lib, без Rust/cargo); examples/tls/echo компилируется и работает; CI гоняет tls+https.

## Границы

http-уровень (методы/keep-alive/streaming) — [178](178-http.md) followups, ортогонально TLS. Этот план — про TLS-транспорт https до прод-готовности.
