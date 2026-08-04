<!-- SPDX-License-Identifier: CC-BY-4.0 -->
# Запуск тестов

[English](running-tests.md) | **Русский**

Соберите `nova` CLI, затем запустите полный набор тестов:

```sh
# build nova CLI (one-time, or after changes)
cd nova-cli && cargo build --release && cd ..

# run all tests
nova-cli/target/release/nova test
```

## Частые флаги

```sh
nova test --filter syntax/closure        # subset of tests
nova test --mode release                 # -O3 -flto compilation
nova test --toolchain clang              # force toolchain
nova test --timeout 60                   # timeout per test
nova test --format json                  # JSON events (one per line)
nova test --format junit > results.xml   # JUnit XML for CI parsers
nova test --retries 2                    # retry transient AV/race fails
nova test --rerun-failed                 # only failed-last-time
nova test --include-stdlib               # include std/* alongside nova_tests/*
```

## Отладка одиночного теста

Без walkdir, без параллельных накладных расходов:

```sh
./compiler-codegen/target/debug/nova-codegen test-build nova_tests/basics/literals.nv \
    --toolchain clang --keep-artifacts
```

## Настройка toolchain'а

- **Windows:** `winget install LLVM.LLVM` (Clang, рекомендуется) +
  Visual Studio Build Tools (MSVC SDK + линкер, нужны и для Clang тоже).
- **Linux:** `apt install clang` или `dnf install clang`; GCC обычно уже
  установлен.
- **macOS:** `xcode-select --install` (Apple Clang).

Автоопределение выбирает сначала Clang, затем MSVC (Windows) или GCC
(Linux). Переопределить: `--toolchain clang|msvc|gcc` или через
переменные окружения (`NOVA_CLANG`, `NOVA_GCC`, `NOVA_VCVARS`).
