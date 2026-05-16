// SPDX-License-Identifier: MIT OR Apache-2.0
//! Plan 57.B.4 — CPU instructions counter (Linux only).
//!
//! Уникальная фича: deterministic measurement (instructions count не
//! зависит от CPU clock, governor, thermal — only от executed code).
//! Не подвержен noise на shared CI runners.
//!
//! Реализация: `perf_event_open` syscall direct via libc FFI.
//! - PERF_TYPE_HARDWARE / PERF_COUNT_HW_INSTRUCTIONS.
//! - perf_event_paranoid <= 1 требуется (или CAP_PERFMON).
//! - Не поддерживается на Windows/macOS — graceful fallback.
//!
//! Reference: man perf_event_open(2).

use anyhow::{anyhow, Result};

#[cfg(target_os = "linux")]
pub mod linux {
    use super::*;
    use std::os::unix::io::RawFd;

    /// perf_event_attr — slim Rust mirror (offsets важны).
    /// См. https://elixir.bootlin.com/linux/latest/source/include/uapi/linux/perf_event.h
    #[repr(C)]
    struct PerfEventAttr {
        type_: u32,
        size: u32,
        config: u64,
        sample_period: u64,
        sample_type: u64,
        read_format: u64,
        flags: u64,        // disabled:1 + inherit:1 + exclude_kernel:1 etc, packed bits
        wakeup_events: u32,
        bp_type: u32,
        bp_addr: u64,
        bp_len: u64,
        branch_sample_type: u64,
        sample_regs_user: u64,
        sample_stack_user: u32,
        clockid: i32,
        sample_regs_intr: u64,
        aux_watermark: u32,
        sample_max_stack: u16,
        __reserved_2: u16,
    }

    const PERF_TYPE_HARDWARE: u32 = 0;
    const PERF_COUNT_HW_INSTRUCTIONS: u64 = 1;
    // Flags bits (packed in PerfEventAttr.flags):
    const PERF_EVENT_ATTR_DISABLED: u64 = 1 << 0;
    const PERF_EVENT_ATTR_INHERIT: u64 = 1 << 1;
    const PERF_EVENT_ATTR_EXCLUDE_KERNEL: u64 = 1 << 5;
    const PERF_EVENT_ATTR_EXCLUDE_HV: u64 = 1 << 6;

    // ioctl
    const PERF_EVENT_IOC_ENABLE:  u64 = 0x2400;
    const PERF_EVENT_IOC_DISABLE: u64 = 0x2401;
    const PERF_EVENT_IOC_RESET:   u64 = 0x2403;

    extern "C" {
        fn syscall(num: i64, ...) -> i64;
        fn ioctl(fd: RawFd, request: u64, ...) -> i32;
        fn read(fd: RawFd, buf: *mut u8, count: usize) -> isize;
        fn close(fd: RawFd) -> i32;
    }

    const SYS_PERF_EVENT_OPEN: i64 = 298;  // x86_64 Linux

    pub struct InstrCounter {
        fd: RawFd,
    }

    impl InstrCounter {
        /// Create counter for current process (pid=0), any CPU (cpu=-1).
        /// Counts instructions in user mode only (exclude_kernel+exclude_hv).
        pub fn new() -> Result<Self> {
            let mut attr = PerfEventAttr {
                type_: PERF_TYPE_HARDWARE,
                size: std::mem::size_of::<PerfEventAttr>() as u32,
                config: PERF_COUNT_HW_INSTRUCTIONS,
                sample_period: 0,
                sample_type: 0,
                read_format: 0,
                flags: PERF_EVENT_ATTR_DISABLED
                     | PERF_EVENT_ATTR_EXCLUDE_KERNEL
                     | PERF_EVENT_ATTR_EXCLUDE_HV,
                wakeup_events: 0,
                bp_type: 0,
                bp_addr: 0,
                bp_len: 0,
                branch_sample_type: 0,
                sample_regs_user: 0,
                sample_stack_user: 0,
                clockid: 0,
                sample_regs_intr: 0,
                aux_watermark: 0,
                sample_max_stack: 0,
                __reserved_2: 0,
            };
            // pid=0 (this process), cpu=-1 (any), group_fd=-1, flags=0.
            let fd = unsafe {
                syscall(
                    SYS_PERF_EVENT_OPEN,
                    &mut attr as *mut PerfEventAttr,
                    0i32,    // pid=0 = current process
                    -1i32,   // cpu=-1 = any
                    -1i32,   // group_fd
                    0u64,    // flags
                ) as RawFd
            };
            if fd < 0 {
                let errno = std::io::Error::last_os_error();
                return Err(anyhow!(
                    "perf_event_open failed: {} (likely /proc/sys/kernel/perf_event_paranoid > 1; \
                     try `sudo sysctl -w kernel.perf_event_paranoid=1`)",
                    errno));
            }
            Ok(Self { fd })
        }

        pub fn reset(&self) -> Result<()> {
            let r = unsafe { ioctl(self.fd, PERF_EVENT_IOC_RESET, 0u64) };
            if r != 0 { return Err(anyhow!("ioctl RESET: {}",
                std::io::Error::last_os_error())); }
            Ok(())
        }

        pub fn start(&self) -> Result<()> {
            let r = unsafe { ioctl(self.fd, PERF_EVENT_IOC_ENABLE, 0u64) };
            if r != 0 { return Err(anyhow!("ioctl ENABLE: {}",
                std::io::Error::last_os_error())); }
            Ok(())
        }

        pub fn stop(&self) -> Result<()> {
            let r = unsafe { ioctl(self.fd, PERF_EVENT_IOC_DISABLE, 0u64) };
            if r != 0 { return Err(anyhow!("ioctl DISABLE: {}",
                std::io::Error::last_os_error())); }
            Ok(())
        }

        pub fn read(&self) -> Result<u64> {
            let mut buf = [0u8; 8];
            let n = unsafe { read(self.fd, buf.as_mut_ptr(), 8) };
            if n != 8 {
                return Err(anyhow!("read counter: short read ({} bytes)", n));
            }
            Ok(u64::from_ne_bytes(buf))
        }
    }

    impl Drop for InstrCounter {
        fn drop(&mut self) {
            unsafe { close(self.fd); }
        }
    }

    /// Check whether perf_event_open is callable (paranoid level + capability).
    pub fn available() -> bool {
        InstrCounter::new().is_ok()
    }
}

/// Cross-platform stub: returns "not available на этой OS".
#[cfg(not(target_os = "linux"))]
pub fn available() -> bool { false }

#[cfg(target_os = "linux")]
pub fn available() -> bool { linux::available() }

/// Measurement wrapper. Returns instruction count for closure execution.
/// Linux: real; other: returns Err(unsupported).
#[cfg(target_os = "linux")]
pub fn measure_instructions<F: FnOnce()>(f: F) -> Result<u64> {
    let counter = linux::InstrCounter::new()?;
    counter.reset()?;
    counter.start()?;
    f();
    counter.stop()?;
    counter.read()
}

#[cfg(not(target_os = "linux"))]
pub fn measure_instructions<F: FnOnce()>(_f: F) -> Result<u64> {
    Err(anyhow!("CPU instructions counter not available on this OS \
                 (Linux only — uses perf_event_open syscall)"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[cfg(target_os = "linux")]
    fn count_basic_loop() {
        if !available() {
            eprintln!("skip: perf_event_open not available (paranoid level?)");
            return;
        }
        let n = measure_instructions(|| {
            let mut x: u64 = 0;
            for _ in 0..1000 { x = x.wrapping_add(1); }
            std::hint::black_box(x);
        }).expect("measure");
        // At least a few thousand instructions.
        assert!(n > 1000, "expected >1000 instructions, got {}", n);
    }

    #[test]
    #[cfg(not(target_os = "linux"))]
    fn unsupported_returns_err() {
        let r = measure_instructions(|| {});
        assert!(r.is_err());
    }
}
