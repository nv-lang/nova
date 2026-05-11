# Промпт: прочитай toolchain

Используй этот промпт когда агент будет **запускать тесты, компилировать
Nova-файлы, добавлять тесты, работать с nova CLI**. Аналог «перечитай спеку»
но для инфраструктуры, не для языка.

---

## Что прочитать

1. **Как работает nova CLI** (`nova-cli/src/main.rs`):
   - `find_repo_root()` — ищет `nova.toml` вверх от CWD.
   - `resolve_paths()` — раскладывает пути (`nova_tests/`, `std/`,
     `compiler-codegen/`, `compiler-codegen/nova_rt/`).
   - Субкоманды: `nova test`, `nova build`, `nova run`, `nova check`,
     `nova regen-runtime`.

2. **Как запускать тесты:**
   ```sh
   cd nova-cli && cargo build && cd ..           # собрать (один раз)
   nova-cli/target/debug/nova test               # все тесты
   nova-cli/target/debug/nova test --filter X    # подмножество
   nova-cli/target/debug/nova test --jobs 1      # sequential (для отладки)
   ```
   Результаты пишутся в `target/last-test-results.json` автоматически.
   Rerun после фикса: `nova-cli/target/debug/nova test --rerun-failed`.

3. **Как добавить тест** (`docs/test-conventions.md`):
   - Создать `.nv` файл в `nova_tests/<group>/`.
   - Первая строка — `module nova_tests.<group>.<name>`.
   - D89 EXPECT-маркер в первых 30 строках (если негативный тест):
     ```nova
     // EXPECT_COMPILE_ERROR some pattern
     // EXPECT_RUNTIME_PANIC some pattern
     // EXPECT_EXIT_CODE 1
     // EXPECT_STDOUT some pattern
     // EXPECT_STDERR some pattern
     ```
   - Без маркера — тест ожидает exit 0.

4. **Как устроен test_runner** (`compiler-codegen/src/test_runner.rs`):
   - `TestAllOpts` — все параметры прогона.
   - `TestAllOpts.toolchain: Toolchain` — **owned**, не borrowed.
     Извлекать `vcvars` из `tc` **до** move в opts.
   - `run_all(opts)` — вызывает `install_cancel_handler()` внутри сам.
   - `detect_or_build_libuv()` — exits process при failure (не возвращает None).
   - `compile_c_to_exe(tc, opts, timeout)` — pub fn для `nova build`.

5. **Как скомпилировать один .nv файл:**
   ```sh
   nova-cli/target/debug/nova build nova_tests/basics/literals.nv
   nova-cli/target/debug/nova run   nova_tests/basics/literals.nv
   nova-cli/target/debug/nova check nova_tests/basics/literals.nv
   ```

6. **Как регенерировать runtime stubs:**
   ```sh
   nova-cli/target/debug/nova regen-runtime          # перезаписать
   nova-cli/target/debug/nova regen-runtime --check  # только проверить (CI)
   ```

7. **Структура репозитория:**
   ```
   compiler-codegen/   ← Rust: компилятор + test runner (nova-codegen binary + nova_codegen lib)
   nova-cli/           ← Rust: пользовательский CLI (nova binary)
   nova_tests/         ← Nova package: тест-корпус (module nova_tests.*)
   std/                ← Nova package: stdlib (module std.*)
   examples/           ← Nova package: примеры
   spec/               ← спека языка
   docs/               ← планы, conventions, promts
   ```
   `nova.toml` — Nova workspace (members: std, examples, nova_tests).
   Два независимых Cargo crate: `compiler-codegen/Cargo.toml` и `nova-cli/Cargo.toml`.

8. **Toolchain detection** (Windows):
   - Clang: `NOVA_CLANG` env или `C:\Program Files\LLVM\bin\clang.exe`.
   - vcvars: `NOVA_VCVARS` env или vswhere auto-detect.
   - GCC: `NOVA_GCC` env или PATH.
   - Auto-order: Clang → MSVC → GCC.

9. **Что делает `nova-codegen` (внутренний инструмент):**
   - `nova-codegen compile file.nv` — Nova → .c
   - `nova-codegen test-build file.nv` — single-test (build + run + EXPECT)
   - `nova-codegen test-all` — batch (используется внутри `nova test`)
   - `nova-codegen emit-runtime-stubs` — то же что `nova regen-runtime`
   Прямой вызов нужен для отладки; пользователь использует `nova`.

---

## Ключевые ловушки

- `TestAllOpts.toolchain` — **owned (move)**, не `&Toolchain`.
  Нужно: `let vcvars = match &tc { Clang { vcvars, .. } => vcvars.clone(), ... };`
  — сохранить vcvars **до** `toolchain: tc` в opts.

- `run_all()` уже вызывает `install_cancel_handler()` внутри.
  Не надо вызывать снаружи.

- `detect_or_build_libuv()` — exits process при failure, не возвращает `None`.

- `retries` в `TestAllOpts` — тип `u32`, не `usize`.

- `--gc malloc` — internal режим (plain malloc без GC), не документируется
  пользователю. После Plan 27 Ф.4 default = boehm GC.

- Module names: `nova_tests.<group>.<file>` — строго по D78.
  Имя директории = имя пакета = prefix модулей.
