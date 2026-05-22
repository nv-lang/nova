//! Plan 03.2 Ф.1 — semver 2.0.0 + version-ranges для `nova.toml`.
//!
//! Bootstrap-Rust, без crate-зависимостей (как `manifest.rs` /
//! `git_cache.rs`). Два типа:
//!
//! - [`Version`] — `MAJOR.MINOR.PATCH` с опциональным `-prerelease`
//!   (semver 2.0.0); `+build`-метаданные при сравнении игнорируются
//!   (semver §10) и не хранятся.
//! - [`VersionReq`] — диапазон версий: `^1.2`, `~1.2.3`, `>=1.0, <2.0`,
//!   `1.2.*`, `*`, `=1.2.3`, голая `1.2.3` (≡ `^1.2.3`, конвенция Cargo).
//!
//! Диапазон внутри — `AND`-список простых компараторов
//! (`>=`/`>`/`<=`/`<`/`=`); caret/tilde/wildcard разворачиваются в
//! пары `>=`/`<` при разборе. `intersect` — конкатенация списков.
//!
//! **Pre-release policy** (паритет с Cargo): диапазон матчит
//! pre-release-версию, только если какой-то его компаратор сам
//! указывает pre-release с тем же `MAJOR.MINOR.PATCH`.

use std::cmp::Ordering;
use std::fmt;

/// Идентификатор pre-release-сегмента: числовой либо алфанумерный.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PreId {
    Num(u64),
    Alpha(String),
}

impl Ord for PreId {
    fn cmp(&self, other: &Self) -> Ordering {
        match (self, other) {
            // semver §11: числовой < алфанумерного.
            (PreId::Num(a), PreId::Num(b)) => a.cmp(b),
            (PreId::Alpha(a), PreId::Alpha(b)) => a.cmp(b),
            (PreId::Num(_), PreId::Alpha(_)) => Ordering::Less,
            (PreId::Alpha(_), PreId::Num(_)) => Ordering::Greater,
        }
    }
}
impl PartialOrd for PreId {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// Версия semver 2.0.0.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Version {
    pub major: u64,
    pub minor: u64,
    pub patch: u64,
    /// Пусто — release-версия (всегда «старше» любого pre-release того
    /// же `MAJOR.MINOR.PATCH`).
    pub pre: Vec<PreId>,
}

impl Version {
    pub fn new(major: u64, minor: u64, patch: u64) -> Version {
        Version { major, minor, patch, pre: Vec::new() }
    }

    /// `true` — версия имеет pre-release-хвост.
    pub fn is_prerelease(&self) -> bool {
        !self.pre.is_empty()
    }

    /// Разобрать строгую semver-строку (`1.2.3`, `1.2.3-rc.1+build`).
    /// Build-метаданные (`+...`) принимаются и отбрасываются.
    pub fn parse(s: &str) -> Result<Version, String> {
        let s = s.trim();
        if s.is_empty() {
            return Err("пустая строка версии".to_string());
        }
        // Отрезать build-метаданные.
        let core_pre = s.split('+').next().unwrap_or(s);
        let (core, pre_str) = match core_pre.split_once('-') {
            Some((c, p)) => (c, Some(p)),
            None => (core_pre, None),
        };
        let mut it = core.split('.');
        let major = parse_num_field(it.next(), "major", s)?;
        let minor = parse_num_field(it.next(), "minor", s)?;
        let patch = parse_num_field(it.next(), "patch", s)?;
        if it.next().is_some() {
            return Err(format!("лишние сегменты в версии `{}`", s));
        }
        let pre = match pre_str {
            None => Vec::new(),
            Some(p) => parse_pre(p, s)?,
        };
        Ok(Version { major, minor, patch, pre })
    }

    /// Разобрать версию из git-тега — допускает ведущий `v`/`V`.
    pub fn parse_tag(s: &str) -> Result<Version, String> {
        let s = s.trim();
        let stripped = s.strip_prefix('v').or_else(|| s.strip_prefix('V')).unwrap_or(s);
        Version::parse(stripped)
    }
}

fn parse_num_field(field: Option<&str>, name: &str, full: &str) -> Result<u64, String> {
    let f = field.ok_or_else(|| format!("версия `{}`: отсутствует {}", full, name))?;
    if f.is_empty() {
        return Err(format!("версия `{}`: пустой сегмент {}", full, name));
    }
    f.parse::<u64>()
        .map_err(|_| format!("версия `{}`: сегмент {} `{}` не число", full, name, f))
}

fn parse_pre(p: &str, full: &str) -> Result<Vec<PreId>, String> {
    if p.is_empty() {
        return Err(format!("версия `{}`: пустой pre-release", full));
    }
    let mut out = Vec::new();
    for seg in p.split('.') {
        if seg.is_empty() {
            return Err(format!("версия `{}`: пустой pre-release-сегмент", full));
        }
        // Числовой сегмент без ведущих нулей → Num, иначе Alpha.
        if seg.chars().all(|c| c.is_ascii_digit()) {
            if seg.len() > 1 && seg.starts_with('0') {
                // Ведущий ноль в числовом pre-release запрещён semver'ом.
                return Err(format!(
                    "версия `{}`: ведущий ноль в pre-release `{}`",
                    full, seg
                ));
            }
            match seg.parse::<u64>() {
                Ok(n) => out.push(PreId::Num(n)),
                Err(_) => out.push(PreId::Alpha(seg.to_string())),
            }
        } else {
            out.push(PreId::Alpha(seg.to_string()));
        }
    }
    Ok(out)
}

impl Ord for Version {
    fn cmp(&self, other: &Self) -> Ordering {
        (self.major, self.minor, self.patch)
            .cmp(&(other.major, other.minor, other.patch))
            .then_with(|| cmp_pre(&self.pre, &other.pre))
    }
}
impl PartialOrd for Version {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// Сравнение pre-release-хвостов (semver §11). Пустой хвост (release)
/// **старше** непустого; иначе — лексикографически по сегментам, более
/// длинный хвост старше при равенстве префикса.
fn cmp_pre(a: &[PreId], b: &[PreId]) -> Ordering {
    match (a.is_empty(), b.is_empty()) {
        (true, true) => Ordering::Equal,
        (true, false) => Ordering::Greater,
        (false, true) => Ordering::Less,
        (false, false) => {
            for (x, y) in a.iter().zip(b.iter()) {
                let c = x.cmp(y);
                if c != Ordering::Equal {
                    return c;
                }
            }
            a.len().cmp(&b.len())
        }
    }
}

impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)?;
        if !self.pre.is_empty() {
            write!(f, "-")?;
            for (i, p) in self.pre.iter().enumerate() {
                if i > 0 {
                    write!(f, ".")?;
                }
                match p {
                    PreId::Num(n) => write!(f, "{}", n)?,
                    PreId::Alpha(s) => write!(f, "{}", s)?,
                }
            }
        }
        Ok(())
    }
}

/// Оператор простого компаратора.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Op {
    Exact,
    Gt,
    Ge,
    Lt,
    Le,
}

/// Простой компаратор: `<op> <version>`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Comparator {
    pub op: Op,
    pub version: Version,
}

impl Comparator {
    fn satisfied_by(&self, v: &Version) -> bool {
        let ord = v.cmp(&self.version);
        match self.op {
            Op::Exact => ord == Ordering::Equal,
            Op::Gt => ord == Ordering::Greater,
            Op::Ge => ord != Ordering::Less,
            Op::Lt => ord == Ordering::Less,
            Op::Le => ord != Ordering::Greater,
        }
    }
}

/// Требование к версии — `AND`-список компараторов. Пустой список
/// (`*`) матчит любую release-версию.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VersionReq {
    pub comparators: Vec<Comparator>,
}

impl VersionReq {
    /// `*` — любая версия.
    pub fn any() -> VersionReq {
        VersionReq { comparators: Vec::new() }
    }

    /// Удовлетворяет ли `v` всем компараторам.
    ///
    /// Pre-release-policy (Cargo): pre-release-версия матчит, только
    /// если какой-то компаратор сам указывает pre-release с тем же
    /// `MAJOR.MINOR.PATCH`.
    pub fn matches(&self, v: &Version) -> bool {
        if v.is_prerelease() && !self.allows_prerelease(v) {
            return false;
        }
        self.comparators.iter().all(|c| c.satisfied_by(v))
    }

    fn allows_prerelease(&self, v: &Version) -> bool {
        self.comparators.iter().any(|c| {
            c.version.is_prerelease()
                && c.version.major == v.major
                && c.version.minor == v.minor
                && c.version.patch == v.patch
        })
    }

    /// Пересечение двух требований — конкатенация `AND`-списков.
    /// Корректно (matches = все компараторы); без канонизации.
    pub fn intersect(&self, other: &VersionReq) -> VersionReq {
        let mut comparators = self.comparators.clone();
        comparators.extend(other.comparators.iter().cloned());
        VersionReq { comparators }
    }

    /// Разобрать строку требования (`^1.2`, `>=1.0, <2.0`, `*`, …).
    pub fn parse(s: &str) -> Result<VersionReq, String> {
        let s = s.trim();
        if s.is_empty() {
            return Err("пустая строка version-требования".to_string());
        }
        let mut comparators = Vec::new();
        for raw in s.split(',') {
            let part = raw.trim();
            if part.is_empty() {
                return Err(format!("требование `{}`: пустой сегмент", s));
            }
            parse_segment(part, &mut comparators)?;
        }
        Ok(VersionReq { comparators })
    }
}

impl fmt::Display for VersionReq {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.comparators.is_empty() {
            return write!(f, "*");
        }
        for (i, c) in self.comparators.iter().enumerate() {
            if i > 0 {
                write!(f, ", ")?;
            }
            let op = match c.op {
                Op::Exact => "=",
                Op::Gt => ">",
                Op::Ge => ">=",
                Op::Lt => "<",
                Op::Le => "<=",
            };
            write!(f, "{}{}", op, c.version)?;
        }
        Ok(())
    }
}

/// Разобрать один сегмент требования в один-два компаратора.
fn parse_segment(part: &str, out: &mut Vec<Comparator>) -> Result<(), String> {
    // `*` — без ограничений.
    if part == "*" {
        return Ok(());
    }
    // Явные операторы сравнения.
    for (prefix, op) in [(">=", Op::Ge), ("<=", Op::Le), (">", Op::Gt), ("<", Op::Lt)] {
        if let Some(rest) = part.strip_prefix(prefix) {
            out.push(Comparator { op, version: parse_partial_full(rest.trim())? });
            return Ok(());
        }
    }
    if let Some(rest) = part.strip_prefix('=') {
        out.push(Comparator { op: Op::Exact, version: Version::parse(rest.trim())? });
        return Ok(());
    }
    // Caret / tilde.
    if let Some(rest) = part.strip_prefix('^') {
        let (lo, hi) = caret_bounds(rest.trim())?;
        out.push(Comparator { op: Op::Ge, version: lo });
        out.push(Comparator { op: Op::Lt, version: hi });
        return Ok(());
    }
    if let Some(rest) = part.strip_prefix('~') {
        let (lo, hi) = tilde_bounds(rest.trim())?;
        out.push(Comparator { op: Op::Ge, version: lo });
        out.push(Comparator { op: Op::Lt, version: hi });
        return Ok(());
    }
    // Wildcard `1.*` / `1.2.*`.
    if part.ends_with(".*") || part == "x" || part == "X" {
        let (lo, hi) = wildcard_bounds(part)?;
        out.push(Comparator { op: Op::Ge, version: lo });
        out.push(Comparator { op: Op::Lt, version: hi });
        return Ok(());
    }
    // Голая версия — конвенция Cargo: `1.2.3` ≡ `^1.2.3`.
    let (lo, hi) = caret_bounds(part)?;
    out.push(Comparator { op: Op::Ge, version: lo });
    out.push(Comparator { op: Op::Lt, version: hi });
    Ok(())
}

/// Разобрать «частичную» версию (`1`, `1.2`, `1.2.3`) — недостающие
/// сегменты → 0. Pre-release допускается только в полной форме.
fn parse_partial(s: &str) -> Result<(u64, Option<u64>, Option<u64>, Vec<PreId>), String> {
    let core_pre = s.split('+').next().unwrap_or(s);
    let (core, pre_str) = match core_pre.split_once('-') {
        Some((c, p)) => (c, Some(p)),
        None => (core_pre, None),
    };
    let segs: Vec<&str> = core.split('.').collect();
    if segs.is_empty() || segs[0].is_empty() {
        return Err(format!("некорректная версия `{}`", s));
    }
    let major = segs[0]
        .parse::<u64>()
        .map_err(|_| format!("версия `{}`: major `{}` не число", s, segs[0]))?;
    let minor = match segs.get(1) {
        None => None,
        Some(m) => Some(
            m.parse::<u64>()
                .map_err(|_| format!("версия `{}`: minor `{}` не число", s, m))?,
        ),
    };
    let patch = match segs.get(2) {
        None => None,
        Some(p) => Some(
            p.parse::<u64>()
                .map_err(|_| format!("версия `{}`: patch `{}` не число", s, p))?,
        ),
    };
    if segs.len() > 3 {
        return Err(format!("версия `{}`: лишние сегменты", s));
    }
    let pre = match pre_str {
        None => Vec::new(),
        Some(p) => parse_pre(p, s)?,
    };
    Ok((major, minor, patch, pre))
}

/// Частичная версия → полная (недостающее = 0). Для `>=`/`<` границ.
fn parse_partial_full(s: &str) -> Result<Version, String> {
    let (major, minor, patch, pre) = parse_partial(s)?;
    Ok(Version {
        major,
        minor: minor.unwrap_or(0),
        patch: patch.unwrap_or(0),
        pre,
    })
}

/// Caret-границы `^X` (Cargo-семантика): «совместимые изменения».
fn caret_bounds(s: &str) -> Result<(Version, Version), String> {
    let (major, minor, patch, pre) = parse_partial(s)?;
    let lo = Version { major, minor: minor.unwrap_or(0), patch: patch.unwrap_or(0), pre };
    let hi = if major > 0 {
        Version::new(major + 1, 0, 0)
    } else if minor.unwrap_or(0) > 0 {
        Version::new(0, minor.unwrap() + 1, 0)
    } else if patch.is_some() {
        // ^0.0.P → >=0.0.P, <0.0.P+1
        Version::new(0, 0, patch.unwrap() + 1)
    } else if minor.is_some() {
        // ^0.0 → >=0.0.0, <0.1.0
        Version::new(0, 1, 0)
    } else {
        // ^0 → >=0.0.0, <1.0.0
        Version::new(1, 0, 0)
    };
    Ok((lo, hi))
}

/// Tilde-границы `~X`: фиксирует major.minor, разрешает patch.
fn tilde_bounds(s: &str) -> Result<(Version, Version), String> {
    let (major, minor, patch, pre) = parse_partial(s)?;
    let lo = Version { major, minor: minor.unwrap_or(0), patch: patch.unwrap_or(0), pre };
    let hi = match minor {
        // ~1.2 / ~1.2.3 → <1.3.0
        Some(m) => Version::new(major, m + 1, 0),
        // ~1 → <2.0.0
        None => Version::new(major + 1, 0, 0),
    };
    Ok((lo, hi))
}

/// Wildcard-границы `1.*` / `1.2.*`.
fn wildcard_bounds(s: &str) -> Result<(Version, Version), String> {
    let body = s.strip_suffix(".*").unwrap_or(s);
    let segs: Vec<&str> = body.split('.').filter(|x| !x.is_empty()).collect();
    let nums: Result<Vec<u64>, String> = segs
        .iter()
        .map(|x| x.parse::<u64>().map_err(|_| format!("wildcard `{}`: `{}` не число", s, x)))
        .collect();
    let nums = nums?;
    match nums.len() {
        // `1.*` → >=1.0.0, <2.0.0
        1 => Ok((Version::new(nums[0], 0, 0), Version::new(nums[0] + 1, 0, 0))),
        // `1.2.*` → >=1.2.0, <1.3.0
        2 => Ok((Version::new(nums[0], nums[1], 0), Version::new(nums[0], nums[1] + 1, 0))),
        _ => Err(format!("некорректный wildcard `{}`", s)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(s: &str) -> Version {
        Version::parse(s).expect("parse version")
    }
    fn req(s: &str) -> VersionReq {
        VersionReq::parse(s).expect("parse req")
    }

    #[test]
    fn version_parse_basic() {
        let x = v("1.2.3");
        assert_eq!((x.major, x.minor, x.patch), (1, 2, 3));
        assert!(!x.is_prerelease());
        assert!(v("1.2.3-rc.1+build.9").is_prerelease());
        assert!(Version::parse("1.2").is_err());
        assert!(Version::parse("1.2.3.4").is_err());
        assert!(Version::parse("x.2.3").is_err());
        assert!(Version::parse("").is_err());
    }

    #[test]
    fn version_parse_tag_strips_v() {
        assert_eq!(Version::parse_tag("v1.2.3").unwrap(), v("1.2.3"));
        assert_eq!(Version::parse_tag("1.2.3").unwrap(), v("1.2.3"));
    }

    #[test]
    fn version_ordering() {
        assert!(v("1.0.0") < v("2.0.0"));
        assert!(v("1.2.0") < v("1.10.0"));
        assert!(v("1.0.1") > v("1.0.0"));
        // pre-release < release того же ядра.
        assert!(v("1.0.0-alpha") < v("1.0.0"));
        assert!(v("1.0.0-alpha") < v("1.0.0-beta"));
        assert!(v("1.0.0-alpha.1") < v("1.0.0-alpha.2"));
        // числовой pre-id < алфанумерного.
        assert!(v("1.0.0-1") < v("1.0.0-alpha"));
        // более длинный хвост старше при равном префиксе.
        assert!(v("1.0.0-alpha") < v("1.0.0-alpha.1"));
    }

    #[test]
    fn req_caret() {
        let r = req("^1.2.3");
        assert!(r.matches(&v("1.2.3")));
        assert!(r.matches(&v("1.9.9")));
        assert!(!r.matches(&v("2.0.0")));
        assert!(!r.matches(&v("1.2.2")));
        // ^0.2.3 → >=0.2.3, <0.3.0
        let z = req("^0.2.3");
        assert!(z.matches(&v("0.2.9")));
        assert!(!z.matches(&v("0.3.0")));
        // ^0.0.3 → >=0.0.3, <0.0.4
        let zz = req("^0.0.3");
        assert!(zz.matches(&v("0.0.3")));
        assert!(!zz.matches(&v("0.0.4")));
    }

    #[test]
    fn req_caret_partial() {
        // ^1.2 → >=1.2.0, <2.0.0
        let r = req("^1.2");
        assert!(r.matches(&v("1.2.0")));
        assert!(r.matches(&v("1.99.0")));
        assert!(!r.matches(&v("2.0.0")));
        // ^1 → >=1.0.0, <2.0.0
        assert!(req("^1").matches(&v("1.5.5")));
        assert!(!req("^1").matches(&v("2.0.0")));
    }

    #[test]
    fn req_tilde() {
        // ~1.2.3 → >=1.2.3, <1.3.0
        let r = req("~1.2.3");
        assert!(r.matches(&v("1.2.9")));
        assert!(!r.matches(&v("1.3.0")));
        // ~1 → >=1.0.0, <2.0.0
        assert!(req("~1").matches(&v("1.9.0")));
        assert!(!req("~1").matches(&v("2.0.0")));
    }

    #[test]
    fn req_wildcard_and_any() {
        assert!(req("*").matches(&v("0.0.1")));
        assert!(req("*").matches(&v("99.0.0")));
        // 1.2.* → >=1.2.0, <1.3.0
        assert!(req("1.2.*").matches(&v("1.2.7")));
        assert!(!req("1.2.*").matches(&v("1.3.0")));
        // 1.* → >=1.0.0, <2.0.0
        assert!(req("1.*").matches(&v("1.9.9")));
        assert!(!req("1.*").matches(&v("2.0.0")));
    }

    #[test]
    fn req_comparators_and_exact() {
        assert!(req("=1.2.3").matches(&v("1.2.3")));
        assert!(!req("=1.2.3").matches(&v("1.2.4")));
        let r = req(">=1.0, <2.0");
        assert!(r.matches(&v("1.5.0")));
        assert!(!r.matches(&v("2.0.0")));
        assert!(!r.matches(&v("0.9.0")));
    }

    #[test]
    fn req_bare_is_caret() {
        // Голая `1.2.3` ≡ `^1.2.3`.
        assert!(req("1.2.3").matches(&v("1.9.0")));
        assert!(!req("1.2.3").matches(&v("2.0.0")));
    }

    #[test]
    fn req_prerelease_policy() {
        // Обычный диапазон НЕ берёт pre-release.
        assert!(!req("^1.2.0").matches(&v("1.5.0-rc.1")));
        assert!(!req(">=1.0.0").matches(&v("2.0.0-alpha")));
        // Если компаратор сам указывает pre-release того же ядра — берёт.
        let r = req(">=1.2.0-rc.1, <2.0.0");
        assert!(r.matches(&v("1.2.0-rc.1")));
        assert!(r.matches(&v("1.2.0-rc.2")));
        assert!(r.matches(&v("1.5.0")));
        // pre-release другого ядра — всё равно не берёт.
        assert!(!r.matches(&v("1.3.0-rc.1")));
    }

    #[test]
    fn req_intersect() {
        let a = req(">=1.0.0");
        let b = req("<2.0.0");
        let both = a.intersect(&b);
        assert!(both.matches(&v("1.5.0")));
        assert!(!both.matches(&v("2.0.0")));
        assert!(!both.matches(&v("0.9.0")));
    }

    #[test]
    fn req_parse_errors() {
        assert!(VersionReq::parse("").is_err());
        assert!(VersionReq::parse(">=1.0,").is_err());
        assert!(VersionReq::parse("^x.y").is_err());
    }
}
