<!-- SPDX-License-Identifier: CC-BY-4.0 -->
# SMT-верификация и настройка Z3

[English](z3-setup.md) | **Русский**

Nova включает статический верификатор контрактов (`requires`/`ensures`/`invariant`).
По умолчанию используется **TrivialBackend** (reflexive tautologies, constant folding) —
работает без внешних зависимостей. Для полноценной верификации нужен **Z3**.

## Без Z3 (по умолчанию)

Работает сразу после обычной сборки. Доказывает только рефлексивные
контракты и константные выражения. Z3-тесты автоматически SKIP.

```bash
cd nova-cli && cargo build --release
nova test nova_tests/contracts/
# PASS: 82  SKIP: 9 (z3-only)
```

## С Z3

**Шаг 1: установить Z3 через vcpkg** (один раз)

```bash
# Windows:
cd compiler-codegen
vcpkg install --triplet x64-windows-static --x-manifest-root=.

# Linux:
cd compiler-codegen
vcpkg install --triplet x64-linux --x-manifest-root=.

# macOS:
cd compiler-codegen
vcpkg install --triplet x64-osx --x-manifest-root=.
```

`vcpkg.json` уже содержит `z3` и `bdwgc` — обе зависимости устанавливаются
одной командой. Результат: `vcpkg_installed/<triplet>/lib/libz3.a`.

> Этот шаг нужен ТОЛЬКО для Z3. Boehm GC (`bdwgc`) он тоже устанавливает —
> если vcpkg уже настроен, `nova build`/`nova test` предпочтут vcpkg-сборку
> (быстрее, без пересборки), — но с #269 Ф.2 это больше не обязательно:
> без vcpkg/`NOVA_GC_LIB_DIR` компилятор одноразово собирает Boehm GC сам
> из вендорённого сабмодуля (`compiler-codegen/nova_rt/gc` +
> `compiler-codegen/nova_rt/libatomic_ops`, тянутся `git clone --recursive`
> или `git submodule update --init`) — см.
> [Сборка из исходников](building-from-source.ru.md).

**Шаг 2: собрать с feature `z3-backend`**

```bash
cd nova-cli
cargo build --release --features z3-backend
```

**Шаг 3: запустить с Z3**

```bash
NOVA_SMT_BACKEND=z3 nova test nova_tests/contracts/
# PASS: 91  SKIP: 0
```

> `VCPKG_TRIPLET` переопределяет triplet если нужен нестандартный
> (например `arm64-linux`).

Подробнее: docs/plans/33-contracts-implementation.md — раздел «Z3 dev-setup».
