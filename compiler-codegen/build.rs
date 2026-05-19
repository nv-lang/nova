//! Plan 33 Z3 milestone: build-script для feature `z3-backend`.
//!
//! Назначение:
//! - Сообщить rustc где искать libz3.lib (vcpkg_installed/x64-windows-static/lib).
//! - Подключить статическую libz3 + её транзитивные системные deps на Windows.
//!
//! Дизайн (см. feedback_third_party_libs):
//! - Никаких сторонних crate (`z3-sys`, `z3`). FFI bindings свои — в
//!   `src/verify/backend/z3_ffi.rs`. Это минимизирует Cargo dep-tree и
//!   удерживает full control над linkage.
//! - Vcpkg manifest (`vcpkg.json`) добавляет `z3` рядом с `bdwgc` —
//!   ту же install-команду что и для bdwgc можно использовать.
//! - При feature off (по умолчанию) — этот build-script no-op,
//!   bootstrap-сборка остаётся «пустой Cargo lockfile + stable Rust 1.85+».
//!
//! NB. Файл намеренно тривиален — пусть Cargo делает свою работу,
//! а мы только указываем где брать lib.

fn main() {
    // Plan 33 V1 closure: linkage активна только когда feature compiled-in.
    // Иначе — bootstrap-сборка не должна требовать libz3 на машине.
    if std::env::var("CARGO_FEATURE_Z3_BACKEND").is_err() {
        return;
    }

    // **Linkage strategy** (Plan 71 follow-up 2026-05-19):
    //
    //   Windows / macOS — vcpkg manifest mode (static):
    //     `<crate>/vcpkg_installed/<triplet>/lib/libz3.a` или `libz3.lib`.
    //     Static чтобы Nova-binary не зависел от внешних DLL.
    //
    //   Linux — система через `apt-get install libz3-dev` (dynamic):
    //     ищем libz3 в стандартных system library paths (`/usr/lib/...`).
    //     Не требуем vcpkg_installed на Linux — apt быстрее и проще для
    //     CI runner. Если vcpkg-build хочется и на Linux — поддерживается
    //     через переопределение `VCPKG_TRIPLET=x64-linux` + наличие
    //     `vcpkg_installed/x64-linux/lib/` (тогда static-link предпочтётся).
    //
    // CI workflow `.github/workflows/contracts-z3.yml` Linux job ставит
    // libz3-dev через apt и ожидает dynamic-link path.

    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    let triplet_default = if cfg!(target_os = "windows") {
        "x64-windows-static"
    } else if cfg!(target_os = "linux") {
        "x64-linux"
    } else if cfg!(target_os = "macos") {
        "x64-osx"
    } else {
        "x64-windows-static"
    };
    let triplet = std::env::var("VCPKG_TRIPLET").unwrap_or_else(|_| triplet_default.into());

    let lib_dir = std::path::PathBuf::from(&manifest_dir)
        .join("vcpkg_installed")
        .join(&triplet)
        .join("lib");

    // Linux: prefer system-installed libz3 (apt-installed libz3-dev) если
    // vcpkg_installed/ нет. Hard-error был бы избыточным — apt-route
    // полностью работоспособна.
    let use_vcpkg = lib_dir.exists();

    if !use_vcpkg && !cfg!(target_os = "linux") {
        // На non-Linux (Windows/macOS) vcpkg обязателен — apt-эквивалент
        // отсутствует, через homebrew/vcpkg идёт стандартный путь.
        panic!(
            "z3-backend feature enabled, but {} not found.\n\
             Run from compiler-codegen/:\n    \
             vcpkg install --triplet {} --x-manifest-root=.\n\
             (see docs/plans/33-contracts-implementation.md Z3 milestone).",
            lib_dir.display(),
            triplet,
        );
    }

    if use_vcpkg {
        println!("cargo:rustc-link-search=native={}", lib_dir.display());
        // На MSVC vcpkg производит `libz3.lib` (с префиксом lib); Rust ищет
        // `<name>.lib`, поэтому `static=libz3` корректен. На Linux/macOS
        // та же библиотека `libz3.a` — `static=z3` (Rust добавит `lib` префикс).
        if cfg!(target_os = "windows") {
            println!("cargo:rustc-link-lib=static=libz3");
        } else {
            println!("cargo:rustc-link-lib=static=z3");
        }
    } else {
        // Linux + no vcpkg = используем system-shared libz3 (apt-installed).
        // libz3.so в /usr/lib/x86_64-linux-gnu/ → стандартный linker path.
        println!("cargo:rustc-link-lib=dylib=z3");
    }

    // Z3 — это C++ библиотека → нужен C++ runtime для статической линковки.
    // На MSVC статический CRT (`/MT`) уже подтягивает msvcrt; для Z3 нужны
    // дополнительные системные libs (psapi.lib — для process info; advapi32 —
    // для crypto/random, используется в Z3_mk_solver).
    if cfg!(target_os = "windows") {
        println!("cargo:rustc-link-lib=dylib=psapi");
        println!("cargo:rustc-link-lib=dylib=advapi32");
        println!("cargo:rustc-link-lib=dylib=user32");
        // C++ runtime — `cargo:rustc-link-lib=dylib=msvcrt` управляется
        // Cargo'ом по `-C target-feature=+crt-static` / профилю; явный
        // libcpmt не нужен пока z3 собран с `-DZ3_BUILD_LIBZ3_SHARED=OFF`
        // (что и делает vcpkg x64-windows-static triplet).
    } else {
        // Linux/macOS: libstdc++/libc++ + threads.
        println!("cargo:rustc-link-lib=dylib=stdc++");
        println!("cargo:rustc-link-lib=dylib=pthread");
    }

    // Plan 33 Z3 milestone: x64-windows-static triplet строит Z3 с /MT.
    // Чтобы избежать CRT mismatch, наш Rust crate должен линковаться с
    // static CRT тоже. Это управляется через `RUSTFLAGS` или `.cargo/config.toml`.
    // Здесь мы только подсказываем через cfg-сообщение в stderr build-script.
    if cfg!(target_os = "windows") {
        println!(
            "cargo:warning=z3-backend uses x64-windows-static triplet (/MT). \
             If linker errors mention LIBCMT vs MSVCRT — set RUSTFLAGS=\"-C target-feature=+crt-static\"."
        );
    }

    println!("cargo:rerun-if-changed=vcpkg.json");
    println!("cargo:rerun-if-changed=build.rs");
}
