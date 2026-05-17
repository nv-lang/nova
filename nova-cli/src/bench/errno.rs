// SPDX-License-Identifier: MIT OR Apache-2.0
//! Plan 57.G.3 — actionable errno decoder для perf_event_open paths.
//!
//! `perf_event_open(2)` failures возвращают raw errno (EACCES/EPERM/
//! ENOENT/ENOSYS/...). Generic `std::io::Error` rendering даёт пользователю
//! только "Permission denied" без подсказки что делать. Этот модуль
//! маппит errno → actionable human text с конкретными командами.
//!
//! Used by: bench/cpu_instr.rs, bench/membw.rs (Linux paths).

use std::io;

/// Сообщение пользователю в дополнение к raw errno.
/// Returns None если errno нет специфичного hint'а — caller использует
/// raw `std::io::Error` рендеринг.
pub fn perf_event_open_hint(err: &io::Error) -> Option<&'static str> {
    let raw = err.raw_os_error()?;
    Some(match raw {
        1   /* EPERM */  => {
            "EPERM — operation not permitted.\n\
             Hint: `perf_event_paranoid` blocks raw hardware events.\n\
               sudo sysctl -w kernel.perf_event_paranoid=1\n\
             OR grant capability:\n\
               sudo setcap cap_perfmon,cap_sys_ptrace+ep <nova-binary>"
        }
        2   /* ENOENT */ => {
            "ENOENT — event not recognized by kernel.\n\
             Hint: requested PMU event не supported на CPU. Check:\n\
               cat /sys/devices/cpu/format/event\n\
             Common cause: uncore_imc/MBM event на older CPU."
        }
        13  /* EACCES */ => {
            "EACCES — access denied.\n\
             Hint: same root cause как EPERM — perf_event_paranoid > 1.\n\
               sudo sysctl -w kernel.perf_event_paranoid=1\n\
             OR run с CAP_PERFMON capability (Linux ≥ 5.8)."
        }
        16  /* EBUSY */  => {
            "EBUSY — PMU counter busy.\n\
             Hint: another process holds the counter (e.g. perf record\n\
             running). Stop competing process и retry."
        }
        22  /* EINVAL */ => {
            "EINVAL — invalid attribute config.\n\
             Hint: PMU event code malformed или incompatible с CPU.\n\
             For raw events check /sys/devices/<pmu>/format/event spec."
        }
        24  /* EMFILE */ => {
            "EMFILE — too many open file descriptors.\n\
             Hint: raise limit `ulimit -n 4096` or close other fds."
        }
        38  /* ENOSYS */ => {
            "ENOSYS — perf_event_open syscall не implemented.\n\
             Hint: kernel built без CONFIG_PERF_EVENTS. На production\n\
             distros (Debian/Ubuntu/Fedora) это almost never the case;\n\
             likely running внутри a stripped container — try host."
        }
        _ => return None,
    })
}

/// Format full error message: original + actionable hint (если есть).
pub fn fmt_perf_event_open_err(prefix: &str, err: &io::Error) -> String {
    match perf_event_open_hint(err) {
        Some(hint) => format!("{}: {}\n\n{}", prefix, err, hint),
        None       => format!("{}: {}", prefix, err),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mk(raw: i32) -> io::Error { io::Error::from_raw_os_error(raw) }

    #[test]
    fn known_errors_have_hints() {
        assert!(perf_event_open_hint(&mk(1)).is_some());   // EPERM
        assert!(perf_event_open_hint(&mk(2)).is_some());   // ENOENT
        assert!(perf_event_open_hint(&mk(13)).is_some());  // EACCES
        assert!(perf_event_open_hint(&mk(16)).is_some());  // EBUSY
        assert!(perf_event_open_hint(&mk(22)).is_some());  // EINVAL
        assert!(perf_event_open_hint(&mk(24)).is_some());  // EMFILE
        assert!(perf_event_open_hint(&mk(38)).is_some());  // ENOSYS
    }

    #[test]
    fn unknown_errno_no_hint() {
        assert!(perf_event_open_hint(&mk(999)).is_none());
    }

    #[test]
    fn hints_contain_actionable_command() {
        assert!(perf_event_open_hint(&mk(1)).unwrap().contains("sysctl"));
        assert!(perf_event_open_hint(&mk(13)).unwrap().contains("sysctl"));
        assert!(perf_event_open_hint(&mk(24)).unwrap().contains("ulimit"));
    }

    #[test]
    fn fmt_includes_prefix_and_hint() {
        let s = fmt_perf_event_open_err("ctx", &mk(1));
        assert!(s.starts_with("ctx:"));
        assert!(s.contains("sysctl"));
    }

    #[test]
    fn fmt_no_hint_falls_back_to_raw() {
        let s = fmt_perf_event_open_err("ctx", &mk(999));
        assert!(s.starts_with("ctx:"));
        assert!(!s.contains("sysctl"));
    }
}
