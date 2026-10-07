//! Межпроцессная блокировка общего каталога кэша (реестр 221.1 №1824).
//!
//! Внутрипроцессные замки (`Mutex`, `lock_for_key`, `VENDOR_FFI_BUILD_LOCK`)
//! видят только ПОТОКИ одного `nova`. Несколько процессов на одном кэше —
//! страж примеров гейта собирает по четыре разом, `make -j`, две сборки в
//! соседних терминалах — делили каталоги без защиты. Замер 2026-10-07: пустой
//! `NOVA_HOME`, четыре параллельных `nova build` — три падают на `git clone`
//! в один `db/<id>.git`; после фикса git-кэша — на сборке vendored mbedTLS в
//! один `native/lib` («[ffi] lib `mbedtls` not found»). Отсюда одна дверь для
//! всех кэшей, которые `nova` достраивает при первом запуске: git-кэш,
//! vendored FFI, libuv, вендорённый bdwgc.
//!
//! Блокировка — ЯДРА ОС (`flock` / `LockFileEx`), а не файл-флажок: её снимает
//! сама ОС, когда процесс умирает, так что брошенной блокировки, которую надо
//! было бы угадывать по возрасту, не бывает. Без новых зависимостей (MSRV 1.85,
//! `File::lock` появился позже). Замок исключающий и ждёт держателя; два
//! дескриптора одного процесса тоже исключают друг друга, поэтому дверь
//! сериализует и потоки.

use std::fs::{File, OpenOptions};
use std::path::Path;

/// Держатель блокировки: отпускается при `drop`.
pub struct FileLock {
    file: File,
}

impl Drop for FileLock {
    fn drop(&mut self) {
        os_unlock(&self.file);
    }
}

/// Взять исключающую блокировку на файле `path` (создаётся, родитель —
/// тоже), дождавшись держателя.
pub fn lock_exclusive(path: &Path) -> std::io::Result<FileLock> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(path)?;
    os_lock_exclusive(&file)?;
    Ok(FileLock { file })
}

#[cfg(unix)]
extern "C" {
    fn flock(fd: i32, operation: i32) -> i32;
}

#[cfg(unix)]
fn os_lock_exclusive(file: &File) -> std::io::Result<()> {
    use std::os::unix::io::AsRawFd;
    const LOCK_EX: i32 = 2;
    loop {
        if unsafe { flock(file.as_raw_fd(), LOCK_EX) } == 0 {
            return Ok(());
        }
        let e = std::io::Error::last_os_error();
        if e.kind() != std::io::ErrorKind::Interrupted {
            return Err(e);
        }
    }
}

#[cfg(unix)]
fn os_unlock(file: &File) {
    use std::os::unix::io::AsRawFd;
    const LOCK_UN: i32 = 8;
    unsafe {
        flock(file.as_raw_fd(), LOCK_UN);
    }
}

#[cfg(windows)]
#[repr(C)]
struct Overlapped {
    internal: usize,
    internal_high: usize,
    offset: u32,
    offset_high: u32,
    h_event: *mut std::ffi::c_void,
}

#[cfg(windows)]
impl Overlapped {
    fn zero() -> Self {
        Overlapped {
            internal: 0,
            internal_high: 0,
            offset: 0,
            offset_high: 0,
            h_event: std::ptr::null_mut(),
        }
    }
}

#[cfg(windows)]
extern "system" {
    fn LockFileEx(
        file: *mut std::ffi::c_void,
        flags: u32,
        reserved: u32,
        bytes_low: u32,
        bytes_high: u32,
        overlapped: *mut Overlapped,
    ) -> i32;
    fn UnlockFileEx(
        file: *mut std::ffi::c_void,
        reserved: u32,
        bytes_low: u32,
        bytes_high: u32,
        overlapped: *mut Overlapped,
    ) -> i32;
}

#[cfg(windows)]
fn os_lock_exclusive(file: &File) -> std::io::Result<()> {
    use std::os::windows::io::AsRawHandle;
    const LOCKFILE_EXCLUSIVE_LOCK: u32 = 2;
    let mut ov = Overlapped::zero();
    // Без LOCKFILE_FAIL_IMMEDIATELY — ждёт, пока держатель не отпустит.
    let ok = unsafe {
        LockFileEx(file.as_raw_handle() as *mut _, LOCKFILE_EXCLUSIVE_LOCK, 0, 1, 0, &mut ov)
    };
    if ok != 0 {
        Ok(())
    } else {
        Err(std::io::Error::last_os_error())
    }
}

#[cfg(windows)]
fn os_unlock(file: &File) {
    use std::os::windows::io::AsRawHandle;
    let mut ov = Overlapped::zero();
    unsafe {
        UnlockFileEx(file.as_raw_handle() as *mut _, 0, 1, 0, &mut ov);
    }
}

#[cfg(not(any(unix, windows)))]
fn os_lock_exclusive(_file: &File) -> std::io::Result<()> {
    Ok(())
}

#[cfg(not(any(unix, windows)))]
fn os_unlock(_file: &File) {}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    /// Два держателя одного файла никогда не внутри одновременно: каждый поток
    /// открывает СВОЙ дескриптор, как отдельный процесс.
    #[test]
    fn exclusive_across_handles() {
        let path = std::env::temp_dir().join(format!(
            "nova_fs_lock_{}_{}.lock",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let inside = Arc::new(AtomicUsize::new(0));
        let overlap = Arc::new(AtomicUsize::new(0));
        let handles: Vec<_> = (0..8)
            .map(|_| {
                let (path, inside, overlap) = (path.clone(), inside.clone(), overlap.clone());
                std::thread::spawn(move || {
                    for _ in 0..20 {
                        let _l = lock_exclusive(&path).expect("lock");
                        if inside.fetch_add(1, Ordering::SeqCst) != 0 {
                            overlap.fetch_add(1, Ordering::SeqCst);
                        }
                        std::thread::sleep(std::time::Duration::from_millis(1));
                        inside.fetch_sub(1, Ordering::SeqCst);
                    }
                })
            })
            .collect();
        for h in handles {
            h.join().unwrap();
        }
        assert_eq!(overlap.load(Ordering::SeqCst), 0, "two holders were inside at once");
        std::fs::remove_file(&path).ok();
    }
}
