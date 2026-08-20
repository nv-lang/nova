---
name: project-z3-backend-ready
description: "Z3 backend в репо уже готов — libz3.lib в vcpkg_installed, не нужно ставить vcpkg"
metadata: 
  node_type: memory
  type: project
  originSessionId: ff21d706-0c31-40b4-ba5a-832db2f2ecee
---

В nova репо **Z3 backend уже установлен**:
- `compiler-codegen/vcpkg_installed/x64-windows-static/lib/libz3.lib` — present.
- Также: gc.lib, atomic_ops.lib, gccpp.lib (для Boehm GC).

**Не нужно** ставить vcpkg / qualify Z3 как unavailable. Если контракт-тесты SKIP'аются с `requires NOVA_SMT_BACKEND=z3 but running with trivial` — это потому что **nova собран без feature**, а не потому что Z3 отсутствует.

**Как использовать:**
```
cd nova-cli
cargo build --release --features z3-backend
$env:NOVA_SMT_BACKEND="z3"
./target/release/nova test nova_tests/contracts/
```

**Why:** забывал это в 2-3 разных моментах сессии 2026-05-18, говорил "Z3 не доступен / требует vcpkg install" хотя нужно было просто пересобрать с feature. Пользователь явно отметил.

**How to apply:** прежде чем сказать "Z3 не доступен" — проверить `ls compiler-codegen/vcpkg_installed/x64-windows-static/lib/libz3.lib`. Если есть → просто rebuild с `--features z3-backend` и set `$env:NOVA_SMT_BACKEND="z3"`.

Связано с [[feedback-read-files-efficiently]] — лучше проверить state репо одной командой, чем делать неверное предположение про environment.
