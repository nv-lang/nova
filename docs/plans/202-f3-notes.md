<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# Plan 202 Ф.3 — чекпоинт (миграция nova-tls на root peers)

**Исполнитель:** sonnet, worktree `nova-p202` (ветка `plan-202-f3`, база `987df76c0`
= main с влитыми Ф.1/Ф.1b/Ф.2). Репа-цель `nova-tls` (ветка `plan-202-f3` от `master`).

## Сделано

1. **nova-tls (git mv, byte-identical содержимое кроме decl-строк):**
   `src/tls/*.nv` → `src/*.nv` (12 файлов), `src/tls/neg/config_ro_frozen_neg.nv`
   → `src/neg/config_ro_frozen_neg.nv`, `src/tls/testdata/` → `src/testdata/`.
   Удалён gitignored build-артефакт `src/tls/cert_modes_test.c` (регенерируется).
   `module tls.tls` → `module tls` во всех 12 root-peer файлах (sed). Импорт в
   `src/neg/config_ro_frozen_neg.nv`: `import tls.tls.{ClientConfig}` →
   `import tls.{ClientConfig}`. `nova.toml` (комментарий) + `README.md`
   (Usage/Layout/Module path/Building standalone разделы) переписаны под root
   peers (D78 rev-4).
2. **Потребители в nova-p202 (ветка `plan-202-f3`):**
   - `std/src/http/error.nv`: `import tls.tls.{TlsError}` → `import tls.{TlsError}`
   - `std/src/http/transport/real.nv`: `import tls.tls.{...}` → `import tls.{...}`
   - `std/nova.toml`: комментарий-пример импорта поправлен
   - `examples/tls/echo_client.nv` / `echo_server.nv`: import + комментарий
   - `examples/tls/README.md`: все упоминания `tls.tls`/`src/tls/testdata`
   Историчекие `docs/plans/193*`, `docs/plans/201*`, `docs/research/2026-07-13-*`,
   `docs/simplifications.md` — НЕ трогал (описывают прошлое состояние/решение,
   переписывать = портить историю; `spec_tests/conformance/d78_root_peers/*` —
   собственная фикстура feature'и, там `tls.tls` — иллюстративный пример статтера
   в комментарии, не реальный импорт).

## Проверки (сжато, детали — в финальном отчёте)

- Обнаружен и обойдён (пересборкой) STALE-бинарь: `nova-cli/target/release/nova.exe`
  на диске не соответствовал HEAD (`git status` чист, но парсер не понимал
  `consume X { if ... }` / `consume X { ro ... }` — репро вне nova-tls,
  `cargo build --release` пересобрал, репро зазеленел). НЕ баг компилятора —
  инфраструктурная стухшесть бинаря в worktree.
- `nova test src` (nova-tls, новый компилятор): **PASS: 1 FAIL: 0** — совпадает
  с ожидаемым baseline из отчёта D188-v3.
- `nova test src --compile-error --positive` (nova-tls): **PASS: 2 FAIL: 0**
  (добавляет neg-фикстуру `config_ro_frozen_neg`).
- Cross-package: `nova check examples/tls/echo_server.nv examples/tls/echo_client.nv`
  (потребитель через `[dependencies] tls = { path = "../../nova-tls" }`,
  `import tls.{...}`) — **PASS 2, только unused-import warnings**.
- `nova check std/src/http/error.nv std/src/http/transport/real.nv` — PASS.
- `nova check std` — 22 FAIL: 21 pre-existing neg/-фикстуры (fail-by-design,
  совпадает с известным baseline) + 1 **не связанный с Ф.3** WIP-провал
  `std/src/net/tcp_share_test.nv` (`E_CONSUME_BLOCK_MOVE_OUT` на
  `s.share()` — Plan 201 «В РАБОТЕ», файл не тронут этим Ф.3-слиянием,
  провалился бы идентично и без моих правок).
- Conformance (весь suite, БЕЗ `--jobs`) — см. финальный отчёт (в процессе на
  момент записи чекпоинта).

## Не сделано / вне объёма

- native/tls_c_shim.c и его подключение — не трогал (задача явно исключила).
- Ретракция rev-3.1 (`internal/`) — Ф.4, отдельная фаза плана.
