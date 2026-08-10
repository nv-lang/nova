//! Plan 186 / D412 — компайл-тайм интринсик `embed("path")`.
//!
//! AST-пасс: каждый вызов `embed("relative/path")` заменяется на
//! `ExprKind::HexBlobLit(<байты файла>)` — дальше по конвейеру (чекер,
//! codegen) блоб неотличим от hex-литерала `x"…"` (единая точка
//! материализации, см. emit_c). Запускается СРАЗУ после
//! resolve_imports_inline (пути peer-файлов уже известны), ДО type-check —
//! чекер никогда не видит вызов неизвестной fn `embed`.
//!
//! Правила (D412):
//! - аргумент — ТОЛЬКО строковый литерал (`E_EMBED_ARG_NOT_STR_LITERAL`);
//! - путь разрешается относительно .nv-файла, где стоит вызов (модель
//!   Rust `include_bytes!`); файл вызова определяется по `span.file_id`
//!   через `module.peer_files`, fallback — entry-файл;
//! - выход из дерева проекта наружу — ошибка (`E_EMBED_OUTSIDE_PROJECT`);
//!   граница — ОБЫЧНО общий CU `project_root`, но per-file_id (Plan 193
//!   Ф.2 gap-2, см. `per_file_embed_root`): peer-файл, физически лежащий
//!   ВНЕ дерева `project_root` (peer из внешней `[dependencies]`
//!   path/git-зависимости, напр. folder=module co-equal `*_test.nv` в
//!   sibling-репе), проверяется против СВОЕЙ СОБСТВЕННОЙ package root
//!   (ближайший `nova.toml`, та же граница что `imports::package_root_of`
//!   даёт относительным импортам) — иначе легитимный `embed(...)` внутри
//!   такой зависимости ловил бы ложный `E_EMBED_OUTSIDE_PROJECT` для
//!   ЛЮБОГО потребителя этой зависимости;
//! - файл не найден / не читается — `E_EMBED_NOT_FOUND`;
//! - список встроенных файлов возвращается caller'у: nova-cli включает
//!   их содержимое в fingerprint кэша сборки (build_cache::compute_c_key) —
//!   правка встроенного файла инвалидирует кэш.

use crate::ast::*;
use crate::diag::{Diagnostic, FileId};
use crate::lints::LintWarning;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Plan 210 (D412-амендмент, ревью-2 §9.1 пункт 5): `resolve_embeds` раньше
/// возвращал ТОЛЬКО `Ok(files)`/`Err(diags)` — не было канала для warning'ов
/// на успешном пути (`embed_dir`'s `W_EMBED_DIR_*` эмитятся ИМЕННО на
/// success — папка резолвится успешно, но с предупреждениями). Добавлен
/// `Vec<LintWarning>` вторым элементом `Ok`-кортежа — тот же тип, что
/// `lints::lint_module` возвращает (переиспользуется существующий
/// `render_with_map`/manual-format канал у каждого caller'а, см. правки в
/// nova-cli/src/main.rs, compiler-codegen/src/main.rs, test_runner.rs).
///
/// Прогоняет embed-резолюцию по модулю. `Ok((files, warnings))` — список
/// успешно встроенных файлов (canonical paths, для fingerprint пересборки)
/// + non-fatal warnings (`W_EMBED_DIR_*`); `Err(diags)` — хотя бы один вызов
/// не разрешился.
pub fn resolve_embeds(
    module: &mut Module,
    entry_path: &Path,
    project_root: &Path,
) -> Result<(Vec<PathBuf>, Vec<LintWarning>), Vec<Diagnostic>> {
    // Карта file_id → каталог файла (для относительного резолва).
    let mut dirs: HashMap<FileId, PathBuf> = HashMap::new();
    for pf in &module.peer_files {
        if let Some(parent) = pf.path.parent() {
            dirs.insert(pf.file_id, parent.to_path_buf());
        }
    }
    let entry_dir = entry_path
        .parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| PathBuf::from("."));
    // Canonical project root — выходы за его пределы запрещены. Если
    // canonicalize не удался (виртуальный корень) — проверка мягко
    // пропускается (файл всё равно должен существовать).
    let canon_root = project_root.canonicalize().ok();

    // Plan 193 Ф.2 gap-2: per-file_id embed-check root (см. module doc-comment
    // + `per_file_embed_root`). Вычисляется для каждого peer-файла (включая
    // entry, который тоже присутствует в `module.peer_files` под
    // `MAIN_FILE_ID` — Plan 42.4); файлы без записи в `roots` (не должно
    // случаться при непустом `peer_files`, но на всякий случай) падают на
    // `entry_root`.
    let mut roots: HashMap<FileId, PathBuf> = HashMap::new();
    for pf in &module.peer_files {
        if let Some(root) = per_file_embed_root(&pf.path, &canon_root) {
            roots.insert(pf.file_id, root);
        }
    }
    let entry_root = per_file_embed_root(entry_path, &canon_root);
    // Plan 210 Ф.6а (D412-амендмент): std-корень для NFC-таблиц (`crate::nfc`)
    // — та же функция, что резолвит std для ЛЮБОГО другого потребителя этого
    // `project_root` (nova-cli/main.rs, compiler-codegen/main.rs — см.
    // call-сайты `resolve_std_path`), так что `embed_resolve` не вводит
    // отдельного механизма поиска std.
    let std_src = crate::manifest::resolve_std_path(project_root);

    let mut ctx = EmbedCtx {
        dirs,
        entry_dir,
        roots,
        entry_root,
        std_src,
        files: Vec::new(),
        diags: Vec::new(),
        warnings: Vec::new(),
    };
    for item in &mut module.items {
        ctx.walk_item(item);
    }
    for pf in &mut module.peer_files {
        for item in &mut pf.items_here {
            ctx.walk_item(item);
        }
    }
    if ctx.diags.is_empty() {
        // Дедуп: один файл может быть встроен многократно.
        ctx.files.sort();
        ctx.files.dedup();
        Ok((ctx.files, ctx.warnings))
    } else {
        // Дедуп диагностик: module.items и peer_files[].items_here — КОПИИ
        // одного AST (Plan 42.4), один битый вызов виден дважды.
        let mut seen: std::collections::HashSet<(crate::diag::Span, String)> =
            std::collections::HashSet::new();
        ctx.diags
            .retain(|d| seen.insert((d.span, d.message.clone())));
        Err(ctx.diags)
    }
}

/// Plan 193 Ф.2 gap-2: embed-check boundary для `file`. Если `file`
/// физически лежит ВНУТРИ `shared_root`'s дерева (обычный случай —
/// entry-файл, его folder-module peers, любой intra-workspace peer) —
/// возвращает `shared_root` НЕИЗМЕНЁННЫМ (существовавшее до gap-2
/// поведение, никакого дрейфа для уже упражнённых кейсов). Иначе (peer
/// пришёл из внешней `[dependencies]` path/git-зависимости — sibling-репа,
/// физически вне `shared_root`) — граница СУЖАЕТСЯ до собственной package
/// root `file`'а (ближайший `nova.toml` вверх по дереву, та же граница,
/// что `imports::package_root_of` даёт относительным импортам) —
/// легитимные `embed(...)` внутри зависимости резолвятся относительно ЕЁ
/// СОБСТВЕННОГО дерева, не дерева импортёра. Если ни `shared_root`, ни
/// package root не резолвятся (виртуальный/несуществующий путь) —
/// fallback на `shared_root.clone()` (тот же soft-skip как раньше: `None`
/// → проверка глушится в `try_replace_embed`).
fn per_file_embed_root(file: &Path, shared_root: &Option<PathBuf>) -> Option<PathBuf> {
    let canon_dir = file.parent().and_then(|p| p.canonicalize().ok());
    if let (Some(dir), Some(root)) = (&canon_dir, shared_root) {
        if dir.starts_with(root) {
            return Some(root.clone());
        }
    }
    crate::imports::package_root_of(file)
        .and_then(|p| p.canonicalize().ok())
        .or_else(|| shared_root.clone())
}

struct EmbedCtx {
    dirs: HashMap<FileId, PathBuf>,
    entry_dir: PathBuf,
    /// Plan 193 Ф.2 gap-2: per-file_id embed-check boundary (см.
    /// `per_file_embed_root`); `None`-запись (файл вообще не резолвил
    /// никакой root) — намеренно НЕ хранится, `root_for` тогда падает на
    /// `entry_root`.
    roots: HashMap<FileId, PathBuf>,
    entry_root: Option<PathBuf>,
    /// Plan 210 Ф.6а: std SOURCE root (`manifest::resolve_std_path`), для
    /// `crate::nfc::normalize_nfc` — `embed_dir`'s NFC-путь-нормализация
    /// читает `<std_src>/unicode/norm_data.nv` (уже сгенерированные Unicode
    /// 16.0 таблицы, Plan 152.4.1) вместо новой Cargo-зависимости.
    std_src: PathBuf,
    files: Vec<PathBuf>,
    diags: Vec<Diagnostic>,
    /// Plan 210 (D412-амендмент): non-fatal `W_EMBED_DIR_*` findings,
    /// накопленные на успешном пути (папка резолвится, но с
    /// предупреждениями — symlink-skip / non-ASCII / large / empty).
    warnings: Vec<LintWarning>,
}

/// Plan 210 (D412-амендмент, ревью-2 §9.1 пункт 3): `\` в исходном
/// строковом литерале пути (`embed`/`embed_dir`) — непортируемый исходник;
/// путь пишется POSIX-стилем `/` независимо от ОС компиляции. Общий
/// helper — один код на обоих интринсиках.
fn check_path_backslash(rel: &str, intrinsic: &str) -> Option<String> {
    if rel.contains('\\') {
        Some(format!(
            "[E_EMBED_PATH_BACKSLASH] the path argument of `{}` must use POSIX `/` \
             separators, not `\\` (`{}`) — paths are portable-source, independent of the \
             compiling OS (D412-amendment, Plan 210)",
            intrinsic, rel
        ))
    } else {
        None
    }
}

/// Plan 210 Ф.7.1 (D412-амендмент, Go-паритет+): один "atom" разобранного
/// glob-паттерна для `embed_dir("dir", glob: "...")`. Работает на `char`, не
/// байтах — безопасно для non-ASCII POSIX-ключей (NFC-нормализованных, Ф.6а).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum GlobAtom {
    /// Буквальный символ (включая `/` — можно написать явный сепаратор).
    Lit(char),
    /// `?` — РОВНО один символ, НЕ `/`.
    Any,
    /// `*` — ноль или больше символов, НИ ОДИН не `/` (не пересекает границу).
    Star,
    /// `**` — ноль или больше символов, МОГУТ включать `/` (пересекает).
    StarStar,
}

fn glob_tokenize(pattern: &str) -> Vec<GlobAtom> {
    let chars: Vec<char> = pattern.chars().collect();
    let mut atoms = Vec::with_capacity(chars.len());
    let mut i = 0;
    while i < chars.len() {
        match chars[i] {
            '*' => {
                if i + 1 < chars.len() && chars[i + 1] == '*' {
                    atoms.push(GlobAtom::StarStar);
                    i += 2;
                } else {
                    atoms.push(GlobAtom::Star);
                    i += 1;
                }
            }
            '?' => {
                atoms.push(GlobAtom::Any);
                i += 1;
            }
            c => {
                atoms.push(GlobAtom::Lit(c));
                i += 1;
            }
        }
    }
    atoms
}

/// Plan 210 Ф.7.1: простой glob-матчер, БЕЗ новых зависимостей (собственная
/// DP над `char`-массивами, O(P*T) — patterns/paths коротки, N файлов
/// ограничен `W_EMBED_DIR_LARGE`-порядком, ни backtracking-взрыва, ни
/// перформанс-риска). Маска сверяется с ОТНОСИТЕЛЬНЫМ POSIX-путём вхождения
/// (тем же ключом, что идёт в `EmbeddedEntry.path` — уже NFC-нормализован,
/// Ф.6а). Семантика (Go-паритет+, задание владельца 2026-07-17):
/// - `*` — ноль+ символов, НЕ пересекает `/` (как `path.Match`/bash без
///   globstar);
/// - `**` — ноль+ символов, ПЕРЕСЕКАЕТ `/` (bash globstar-стиль);
/// - `?` — ровно один символ, не `/`;
/// - любой другой символ — буквальный (включая явный `/` в паттерне).
/// **Известное упрощение** (документировано в плане/спеке): `**` не даёт
/// bash'овского "нулевой-каталог" сокращения — `"**/*.png"` требует
/// буквальный `/` в тексте и НЕ матчит файл в корне (`a.png`); для матча и
/// корня, и вложенности используйте `"*.png"` (не пересекает, но зато сам
/// корень — это "нулевая глубина") или разделяйте на два вызова.
fn glob_match_posix(pattern: &str, text: &str) -> bool {
    let atoms = glob_tokenize(pattern);
    let t: Vec<char> = text.chars().collect();
    let n = t.len();
    // dp[j] == atoms[0..i] matches t[0..j] (i — текущая обрабатываемая
    // строка, живёт только в `dp`/`new_dp`, не материализуется как 2D-массив).
    let mut dp = vec![false; n + 1];
    dp[0] = true; // atoms[0..0] (пустой паттерн) матчит только пустой текст.
    for atom in &atoms {
        let mut new_dp = vec![false; n + 1];
        match *atom {
            GlobAtom::Lit(c) => {
                for j in 1..=n {
                    new_dp[j] = dp[j - 1] && t[j - 1] == c;
                }
            }
            GlobAtom::Any => {
                for j in 1..=n {
                    new_dp[j] = dp[j - 1] && t[j - 1] != '/';
                }
            }
            GlobAtom::Star => {
                new_dp[0] = dp[0];
                for j in 1..=n {
                    new_dp[j] = dp[j] || (t[j - 1] != '/' && new_dp[j - 1]);
                }
            }
            GlobAtom::StarStar => {
                new_dp[0] = dp[0];
                for j in 1..=n {
                    new_dp[j] = dp[j] || new_dp[j - 1];
                }
            }
        }
        dp = new_dp;
    }
    dp[n]
}

impl EmbedCtx {
    fn base_dir(&self, file_id: FileId) -> &Path {
        self.dirs
            .get(&file_id)
            .map(|p| p.as_path())
            .unwrap_or(self.entry_dir.as_path())
    }

    /// Plan 193 Ф.2 gap-2: embed-check root для конкретного вызова, по
    /// `file_id` вызывающего файла (`e.span.file_id`) — см. `roots` doc.
    fn root_for(&self, file_id: FileId) -> Option<&Path> {
        self.roots
            .get(&file_id)
            .map(|p| p.as_path())
            .or(self.entry_root.as_deref())
    }

    /// Если `e` — вызов `embed(...)`, заменить его на HexBlobLit (или
    /// накопить диагностику). Возвращает true, если узел был embed-вызовом
    /// (детей у него больше нет — рекурсия не нужна).
    fn try_replace_embed(&mut self, e: &mut Expr) -> bool {
        let ExprKind::Call { func, args, trailing } = &e.kind else {
            return false;
        };
        let ExprKind::Ident(name) = &func.kind else {
            return false;
        };
        if name != "embed" {
            return false;
        }
        if trailing.is_some() || args.len() != 1 {
            self.diags.push(Diagnostic::new(
                "[E_EMBED_ARG_NOT_STR_LITERAL] `embed` takes exactly one string-literal \
                 argument: embed(\"relative/path\") (D412)",
                e.span,
            ));
            return true;
        }
        let rel: String = match &args[0] {
            CallArg::Item(a) => match &a.kind {
                ExprKind::StrLit(s) => s.clone(),
                _ => {
                    self.diags.push(Diagnostic::new(
                        "[E_EMBED_ARG_NOT_STR_LITERAL] the argument of `embed` must be a \
                         string LITERAL — the path is resolved at compile time (D412)",
                        a.span,
                    ));
                    return true;
                }
            },
            _ => {
                self.diags.push(Diagnostic::new(
                    "[E_EMBED_ARG_NOT_STR_LITERAL] the argument of `embed` must be a \
                     plain string literal (no spread/named args) (D412)",
                    e.span,
                ));
                return true;
            }
        };
        if let Some(msg) = check_path_backslash(&rel, "embed") {
            self.diags.push(Diagnostic::new(msg, e.span));
            return true;
        }
        let base = self.base_dir(e.span.file_id).to_path_buf();
        let candidate = base.join(&rel);
        let canon = match candidate.canonicalize() {
            Ok(c) => c,
            Err(_) => {
                self.diags.push(Diagnostic::new(
                    format!(
                        "[E_EMBED_NOT_FOUND] embedded file not found: `{}` (resolved \
                         relative to `{}`)",
                        rel,
                        base.display()
                    ),
                    e.span,
                ));
                return true;
            }
        };
        // Plan 210 (D412-амендмент, ревью-2 §9.1): symmetry with `embed_dir`'s
        // `E_EMBED_NOT_A_DIR` — `embed("папка")` used to fall through to a
        // generic read-failure `E_EMBED_NOT_FOUND` message. Dedicated code +
        // suggestion.
        if canon.is_dir() {
            self.diags.push(Diagnostic::new(
                format!(
                    "[E_EMBED_IS_A_DIR] `{}` is a directory, not a file — use \
                     `embed_dir(...)` instead of `embed(...)` (D412-amendment, Plan 210)",
                    canon.display()
                ),
                e.span,
            ));
            return true;
        }
        if let Some(root) = self.root_for(e.span.file_id) {
            if !canon.starts_with(root) {
                self.diags.push(Diagnostic::new(
                    format!(
                        "[E_EMBED_OUTSIDE_PROJECT] embedded file `{}` escapes the project \
                         root `{}` — paths above the project tree are forbidden (D412)",
                        canon.display(),
                        root.display()
                    ),
                    e.span,
                ));
                return true;
            }
        }
        match std::fs::read(&canon) {
            Ok(bytes) => {
                e.kind = ExprKind::HexBlobLit(bytes);
                self.files.push(canon);
            }
            Err(err) => {
                self.diags.push(Diagnostic::new(
                    format!(
                        "[E_EMBED_NOT_FOUND] cannot read embedded file `{}`: {}",
                        canon.display(),
                        err
                    ),
                    e.span,
                ));
            }
        }
        true
    }

    /// Plan 210 Ф.7.2 (Go-паритет+, D412-амендмент, 2026-07-17): `embed_str("file")`
    /// — компайл-тайм интринсик, содержимое файла как `str` (UTF-8-валидированное).
    /// Зеркало `try_replace_embed` (not_found/is_dir/escape/backslash — ТЕ ЖЕ коды);
    /// собственная валидация — файл ОБЯЗАН быть валидным UTF-8 (`str`-инвариант
    /// компилятора требует этого; невалидный байт → `E_EMBED_NOT_UTF8` с offset'ом
    /// первого битого байта). Синтез — `ExprKind::StrLit(text)`: та же интернирующая
    /// инфраструктура строковых литералов (`intern_str_literal`, emit_c.rs 55020),
    /// что и любой рукописный `"..."` — 0 правок emit_c (симметрично `embed`/
    /// `embed_dir`'s `HexBlobLit`-переиспользованию, §1 плана).
    fn try_replace_embed_str(&mut self, e: &mut Expr) -> bool {
        let ExprKind::Call { func, args, trailing } = &e.kind else {
            return false;
        };
        let ExprKind::Ident(name) = &func.kind else {
            return false;
        };
        if name != "embed_str" {
            return false;
        }
        if trailing.is_some() || args.len() != 1 {
            self.diags.push(Diagnostic::new(
                "[E_EMBED_ARG_NOT_STR_LITERAL] `embed_str` takes exactly one string-literal \
                 argument: embed_str(\"relative/path\") (D412-amendment, Plan 210 Ф.7.2)",
                e.span,
            ));
            return true;
        }
        let rel: String = match &args[0] {
            CallArg::Item(a) => match &a.kind {
                ExprKind::StrLit(s) => s.clone(),
                _ => {
                    self.diags.push(Diagnostic::new(
                        "[E_EMBED_ARG_NOT_STR_LITERAL] the argument of `embed_str` must be a \
                         string LITERAL — the path is resolved at compile time \
                         (D412-amendment, Plan 210 Ф.7.2)",
                        a.span,
                    ));
                    return true;
                }
            },
            _ => {
                self.diags.push(Diagnostic::new(
                    "[E_EMBED_ARG_NOT_STR_LITERAL] the argument of `embed_str` must be a \
                     plain string literal (no spread/named args) (D412-amendment, Plan 210 Ф.7.2)",
                    e.span,
                ));
                return true;
            }
        };
        if let Some(msg) = check_path_backslash(&rel, "embed_str") {
            self.diags.push(Diagnostic::new(msg, e.span));
            return true;
        }
        let base = self.base_dir(e.span.file_id).to_path_buf();
        let candidate = base.join(&rel);
        let canon = match candidate.canonicalize() {
            Ok(c) => c,
            Err(_) => {
                self.diags.push(Diagnostic::new(
                    format!(
                        "[E_EMBED_NOT_FOUND] embedded file not found: `{}` (resolved \
                         relative to `{}`)",
                        rel,
                        base.display()
                    ),
                    e.span,
                ));
                return true;
            }
        };
        if canon.is_dir() {
            self.diags.push(Diagnostic::new(
                format!(
                    "[E_EMBED_IS_A_DIR] `{}` is a directory, not a file — use \
                     `embed_dir(...)` instead of `embed_str(...)` (D412-amendment, Plan 210 Ф.7.2)",
                    canon.display()
                ),
                e.span,
            ));
            return true;
        }
        if let Some(root) = self.root_for(e.span.file_id) {
            if !canon.starts_with(root) {
                self.diags.push(Diagnostic::new(
                    format!(
                        "[E_EMBED_OUTSIDE_PROJECT] embedded file `{}` escapes the project \
                         root `{}` — paths above the project tree are forbidden (D412)",
                        canon.display(),
                        root.display()
                    ),
                    e.span,
                ));
                return true;
            }
        }
        let bytes = match std::fs::read(&canon) {
            Ok(b) => b,
            Err(err) => {
                self.diags.push(Diagnostic::new(
                    format!(
                        "[E_EMBED_NOT_FOUND] cannot read embedded file `{}`: {}",
                        canon.display(),
                        err
                    ),
                    e.span,
                ));
                return true;
            }
        };
        match String::from_utf8(bytes) {
            Ok(text) => {
                e.kind = ExprKind::StrLit(text);
                self.files.push(canon);
            }
            Err(err) => {
                let offset = err.utf8_error().valid_up_to();
                self.diags.push(Diagnostic::new(
                    format!(
                        "[E_EMBED_NOT_UTF8] embedded file `{}` is not valid UTF-8 (first \
                         invalid byte at offset {}) — `embed_str` requires text content; use \
                         `embed(...)` for raw bytes (D412-amendment, Plan 210 Ф.7.2)",
                        canon.display(),
                        offset
                    ),
                    e.span,
                ));
            }
        }
        true
    }

    /// Plan 210 (D412-амендмент): `embed_dir("dir")` → синтезированный
    /// `Call{ EmbeddedDir.new([RecordLit{EmbeddedEntry, path, data}, …]) }`
    /// (Option R′, §3/§4.2 плана). Зеркало `try_replace_embed`, вызывается
    /// из `walk_expr` рядом с ним. ТОЛЬКО free-Ident-форма вызова
    /// (`embed_dir(...)`) — `x.embed_dir(...)` (Member-позиция) НЕ
    /// перехватывается (имя не Ident, а Member.name — ветка ниже уже это
    /// гарантирует симметрично `try_replace_embed`).
    fn try_replace_embed_dir(&mut self, e: &mut Expr) -> bool {
        let ExprKind::Call { func, args, trailing } = &e.kind else {
            return false;
        };
        let ExprKind::Ident(name) = &func.kind else {
            return false;
        };
        if name != "embed_dir" {
            return false;
        }
        if trailing.is_some() || args.is_empty() {
            self.diags.push(Diagnostic::new(
                "[E_EMBED_ARG_NOT_STR_LITERAL] `embed_dir` takes one string-literal path \
                 argument, plus optional named args `glob`/`hidden`: \
                 embed_dir(\"relative/dir\", glob: \"*.png\", hidden: true) \
                 (D412-amendment, Plan 210 Ф.7)",
                e.span,
            ));
            return true;
        }
        let rel: String = match &args[0] {
            CallArg::Item(a) => match &a.kind {
                ExprKind::StrLit(s) => s.clone(),
                _ => {
                    self.diags.push(Diagnostic::new(
                        "[E_EMBED_ARG_NOT_STR_LITERAL] the argument of `embed_dir` must be a \
                         string LITERAL — the path is resolved at compile time \
                         (D412-amendment, Plan 210)",
                        a.span,
                    ));
                    return true;
                }
            },
            _ => {
                self.diags.push(Diagnostic::new(
                    "[E_EMBED_ARG_NOT_STR_LITERAL] the first argument of `embed_dir` must be a \
                     plain positional string literal (no spread/named for the path) \
                     (D412-amendment, Plan 210)",
                    e.span,
                ));
                return true;
            }
        };
        if let Some(msg) = check_path_backslash(&rel, "embed_dir") {
            self.diags.push(Diagnostic::new(msg, e.span));
            return true;
        }
        // Plan 210 Ф.7.1/Ф.7.3 (Go-паритет+, 2026-07-17): именованные аргументы
        // `glob`/`hidden` после позиционного пути. И.7.7 обеих: любой аргумент
        // ПОСЛЕ индекса 0 обязан быть `CallArg::Named` с известным именем;
        // лишний позиционный / spread / неизвестное имя / дубль имени —
        // `E_EMBED_DIR_BAD_ARG` (единый код для этой малой семьи форм-ошибок,
        // симметрично тому, как `E_EMBED_ARG_NOT_STR_LITERAL` уже покрывает
        // несколько сценариев одним кодом).
        let mut glob_pat: Option<String> = None;
        let mut hidden_flag: bool = false;
        let mut glob_seen = false;
        let mut hidden_seen = false;
        for extra in &args[1..] {
            match extra {
                CallArg::Named { name: arg_name, value } => match arg_name.as_str() {
                    "glob" => {
                        if glob_seen {
                            self.diags.push(Diagnostic::new(
                                "[E_EMBED_DIR_BAD_ARG] `embed_dir`: named argument `glob` \
                                 given more than once (D412-amendment, Plan 210 Ф.7.1)",
                                e.span,
                            ));
                            return true;
                        }
                        glob_seen = true;
                        match &value.kind {
                            ExprKind::StrLit(s) => glob_pat = Some(s.clone()),
                            _ => {
                                self.diags.push(Diagnostic::new(
                                    "[E_EMBED_ARG_NOT_STR_LITERAL] `embed_dir`'s `glob:` \
                                     argument must be a string LITERAL (matched at compile \
                                     time) (D412-amendment, Plan 210 Ф.7.1)",
                                    value.span,
                                ));
                                return true;
                            }
                        }
                    }
                    "hidden" => {
                        if hidden_seen {
                            self.diags.push(Diagnostic::new(
                                "[E_EMBED_DIR_BAD_ARG] `embed_dir`: named argument `hidden` \
                                 given more than once (D412-amendment, Plan 210 Ф.7.3)",
                                e.span,
                            ));
                            return true;
                        }
                        hidden_seen = true;
                        match &value.kind {
                            ExprKind::BoolLit(b) => hidden_flag = *b,
                            _ => {
                                self.diags.push(Diagnostic::new(
                                    "[E_EMBED_DIR_BAD_ARG] `embed_dir`'s `hidden:` argument \
                                     must be a bool LITERAL (`true`/`false`) \
                                     (D412-amendment, Plan 210 Ф.7.3)",
                                    value.span,
                                ));
                                return true;
                            }
                        }
                    }
                    other => {
                        self.diags.push(Diagnostic::new(
                            format!(
                                "[E_EMBED_DIR_BAD_ARG] `embed_dir`: unknown named argument \
                                 `{}` — expected `glob` or `hidden` (D412-amendment, \
                                 Plan 210 Ф.7)",
                                other
                            ),
                            e.span,
                        ));
                        return true;
                    }
                },
                CallArg::Item(_) | CallArg::Spread(_) => {
                    self.diags.push(Diagnostic::new(
                        "[E_EMBED_DIR_BAD_ARG] `embed_dir` takes exactly one POSITIONAL \
                         argument (the path) — extra arguments must be named (`glob:`/`hidden:`) \
                         (D412-amendment, Plan 210 Ф.7)",
                        e.span,
                    ));
                    return true;
                }
            }
        }
        let base = self.base_dir(e.span.file_id).to_path_buf();
        let candidate = base.join(&rel);
        let canon = match candidate.canonicalize() {
            Ok(c) => c,
            Err(_) => {
                self.diags.push(Diagnostic::new(
                    format!(
                        "[E_EMBED_DIR_NOT_FOUND] embedded directory not found: `{}` \
                         (resolved relative to `{}`)",
                        rel,
                        base.display()
                    ),
                    e.span,
                ));
                return true;
            }
        };
        if !canon.is_dir() {
            self.diags.push(Diagnostic::new(
                format!(
                    "[E_EMBED_NOT_A_DIR] `{}` is a file, not a directory — use `embed(...)` \
                     instead of `embed_dir(...)` (D412-amendment, Plan 210)",
                    canon.display()
                ),
                e.span,
            ));
            return true;
        }
        let root_opt: Option<PathBuf> =
            self.root_for(e.span.file_id).map(|p| p.to_path_buf());
        if let Some(root) = &root_opt {
            if !canon.starts_with(root) {
                self.diags.push(Diagnostic::new(
                    format!(
                        "[E_EMBED_OUTSIDE_PROJECT] embedded directory `{}` escapes the \
                         project root `{}` — paths above the project tree are forbidden \
                         (D412-amendment, Plan 210)",
                        canon.display(),
                        root.display()
                    ),
                    e.span,
                ));
                return true;
            }
        }

        // Обход (§2е/§4.2): рекурсивный, dot-skip (кроме явно названного
        // корня — сам `canon` уже резолвлен выше, dot-skip применяется
        // ТОЛЬКО к записям, встреченным ВНУТРИ обхода), symlink-skip+warn,
        // non-ASCII-warn. Финальная сортировка — по POSIX-байтам (ниже).
        let mut collected: Vec<(String, PathBuf)> = Vec::new();
        let mut symlinks_skipped: Vec<String> = Vec::new();
        let mut non_ascii_names: Vec<String> = Vec::new();
        // Plan 210 Ф.7.3 (Go-паритет+): `hidden: true` отключает dot-skip для
        // ЗАПИСЕЙ ВНУТРИ обхода (симлинки продолжают скипаться безусловно —
        // `walk_embed_dir_rec` не принимает флага для этого, см. его doc).
        walk_embed_dir_rec(
            &canon,
            &canon,
            &mut collected,
            &mut symlinks_skipped,
            &mut non_ascii_names,
            hidden_flag,
        );

        // Plan 210 Ф.6а (D412-амендмент): NFC-нормализация КАЖДОГО пути записи
        // — воспроизводимость между macOS (обычно отдаёт NFD) и Windows/Linux
        // (обычно NFC): один и тот же чекаут раньше давал разные байтовые
        // ключи (и разный `.c`) на разных ОС; теперь ключ всегда канонический
        // NFC. Коллизия форм (два РАЗНЫХ исходных пути → одна NFC-форма) —
        // `E_EMBED_DIR_NFC_COLLISION` (жёсткая ошибка — тихая перезапись одной
        // записи другой в отсортированной таблице была бы хуже молчаливого
        // предупреждения). Нормализуется ЦЕЛИКОМ относительный POSIX-путь, не
        // по компоненту: `/` не участвует ни в одной canonical
        // decomposition/composition паре Unicode, так что оба варианта дают
        // идентичный результат — целиком проще (`crate::nfc::normalize_nfc`).
        let mut nfc_seen: HashMap<String, String> = HashMap::new(); // nfc_key -> первый увиденный raw-путь
        let mut nfc_normalized: Vec<(String, PathBuf)> = Vec::with_capacity(collected.len());
        for (raw, abs) in &collected {
            let nfc_key = crate::nfc::normalize_nfc(&self.std_src, raw);
            match nfc_seen.get(&nfc_key) {
                Some(prev_raw) if prev_raw != raw => {
                    self.diags.push(Diagnostic::new(
                        format!(
                            "[E_EMBED_DIR_NFC_COLLISION] embed_dir(\"{}\") has two different \
                             entries that normalize to the same NFC path `{}`: `{}` and `{}` — \
                             rename one of them (D412-amendment, Plan 210)",
                            rel, nfc_key, prev_raw, raw
                        ),
                        e.span,
                    ));
                    return true;
                }
                _ => {
                    nfc_seen.insert(nfc_key.clone(), raw.clone());
                }
            }
            nfc_normalized.push((nfc_key, abs.clone()));
        }
        let collected = nfc_normalized;

        // Plan 210 Ф.7.1 (Go-паритет+): `glob:` фильтрует результат обхода
        // ПОСЛЕ dot/symlink-skip и NFC-нормализации — маска сверяется с
        // ФИНАЛЬНЫМ POSIX-ключом (тем же, что попадёт в `EmbeddedEntry.path`).
        // `glob` и `hidden` НЕЗАВИСИМЫ: `hidden` решает, что ПОПАДАЕТ в обход;
        // `glob` фильтрует уже собранный результат — dot-skipped записи (когда
        // `hidden` не включён) glob не может "вернуть".
        let collected: Vec<(String, PathBuf)> = match &glob_pat {
            Some(pat) => collected
                .into_iter()
                .filter(|(path, _)| glob_match_posix(pat, path))
                .collect(),
            None => collected,
        };

        // Per-file escape re-check (§2и: симлинк внутри мог бы указать
        // наружу — симлинки уже скипнуты выше, это defense-in-depth раз
        // обход итерирует строго под `canon`).
        if let Some(root) = &root_opt {
            for (_, abs) in &collected {
                if !abs.starts_with(root) {
                    self.diags.push(Diagnostic::new(
                        format!(
                            "[E_EMBED_OUTSIDE_PROJECT] embedded file `{}` escapes the \
                             project root `{}` (D412-amendment, Plan 210)",
                            abs.display(),
                            root.display()
                        ),
                        e.span,
                    ));
                    return true;
                }
            }
        }

        // Читаем байты (после того как все обойдённые файлы прошли
        // границы) — ошибка чтения → E_EMBED_DIR_NOT_FOUND (race:
        // файл исчез между walk и read).
        let mut entries: Vec<(String, Vec<u8>)> = Vec::with_capacity(collected.len());
        let mut total_bytes: u64 = 0;
        for (rel_posix, abs) in &collected {
            match std::fs::read(abs) {
                Ok(bytes) => {
                    total_bytes += bytes.len() as u64;
                    entries.push((rel_posix.clone(), bytes));
                }
                Err(err) => {
                    self.diags.push(Diagnostic::new(
                        format!(
                            "[E_EMBED_DIR_NOT_FOUND] cannot read embedded file `{}`: {}",
                            abs.display(),
                            err
                        ),
                        e.span,
                    ));
                    return true;
                }
            }
        }
        // Детерминизм (§2а'): сортировка по UTF-8 байтовому порядку пути ==
        // порядок `str.compare` (D178) — предпосылка корректности бинарного
        // поиска в `EmbeddedDir.@get`.
        entries.sort_by(|a, b| a.0.as_bytes().cmp(b.0.as_bytes()));

        // Warnings (§2е/§4.3) — накапливаются на self.warnings, non-fatal.
        for path in &symlinks_skipped {
            self.warnings.push(LintWarning {
                rule: "W_EMBED_DIR_SYMLINK_SKIPPED",
                diag: Diagnostic::new(
                    format!(
                        "[W_EMBED_DIR_SYMLINK_SKIPPED] symlink skipped inside \
                         embed_dir(\"{}\"): `{}`",
                        rel, path
                    ),
                    e.span,
                ),
            });
        }
        for name in &non_ascii_names {
            self.warnings.push(LintWarning {
                rule: "W_EMBED_DIR_NON_ASCII_PATH",
                diag: Diagnostic::new(
                    format!(
                        "[W_EMBED_DIR_NON_ASCII_PATH] non-ASCII file name in \
                         embed_dir(\"{}\"): `{}` — key is normalized to NFC for cross-OS \
                         reproducibility (macOS commonly gives NFD, Windows/Linux NFC); a \
                         form-collision with another entry is a hard error \
                         (E_EMBED_DIR_NFC_COLLISION), not a silent overwrite \
                         (D412-amendment, Plan 210 Ф.6а)",
                        rel, name
                    ),
                    e.span,
                ),
            });
        }
        // §9.2 ревью-3: порог понижен 64→16 MiB (hex-blob текстовый рендер
        // ×5.3 — 64 MiB payload дал бы ~340 МБ .c).
        const LARGE_BYTES: u64 = 16 * 1024 * 1024;
        const LARGE_COUNT: usize = 4096;
        if total_bytes > LARGE_BYTES || entries.len() > LARGE_COUNT {
            self.warnings.push(LintWarning {
                rule: "W_EMBED_DIR_LARGE",
                diag: Diagnostic::new(
                    format!(
                        "[W_EMBED_DIR_LARGE] embed_dir(\"{}\") embeds {} file(s), {} bytes \
                         total — hex-blob text rendering expands ~5.3x in the generated .c; \
                         consider trimming the embedded tree",
                        rel,
                        entries.len(),
                        total_bytes
                    ),
                    e.span,
                ),
            });
        }
        if entries.is_empty() {
            self.warnings.push(LintWarning {
                rule: "W_EMBED_DIR_EMPTY",
                diag: Diagnostic::new(
                    format!(
                        "[W_EMBED_DIR_EMPTY] embed_dir(\"{}\") resolved to zero files after \
                         dot/symlink skip{} — check the path{}",
                        rel,
                        if glob_pat.is_some() { " and glob filter" } else { "" },
                        match &glob_pat {
                            Some(g) => format!(" (glob: \"{}\")", g),
                            None => String::new(),
                        }
                    ),
                    e.span,
                ),
            });
        }

        // Fingerprint (§2ж): ВСЕ обойдённые файлы — зависимости сборки.
        for (_, abs) in &collected {
            self.files.push(abs.clone());
        }

        // Синтез (§3 Option R′): Call{ EmbeddedDir.new([RecordLit{EmbeddedEntry,
        // path, data}, …]) } — все под-узлы делят span вызова `embed_dir`
        // (зеркало `try_replace_embed`'s HexBlobLit-замены).
        let span = e.span;
        let array_items: Vec<ArrayElem> = entries
            .into_iter()
            .map(|(path, bytes)| {
                let path_field = RecordLitField {
                    name: "path".to_string(),
                    value: Some(Expr::new(ExprKind::StrLit(path), span)),
                    is_spread: false,
                    at_shorthand: false,
                    span,
                };
                let data_field = RecordLitField {
                    name: "data".to_string(),
                    value: Some(Expr::new(ExprKind::HexBlobLit(bytes), span)),
                    is_spread: false,
                    at_shorthand: false,
                    span,
                };
                ArrayElem::Item(Expr::new(
                    ExprKind::RecordLit {
                        type_name: Some(vec!["EmbeddedEntry".to_string()]),
                        fields: vec![path_field, data_field],
                        inferred_map_v: None,
                        inferred_target_type: None,
                    },
                    span,
                ))
            })
            .collect();
        let array_expr = Expr::new(ExprKind::ArrayLit(array_items), span);
        let ctor_path = Expr::new(
            ExprKind::Path(vec!["EmbeddedDir".to_string(), "new".to_string()]),
            span,
        );
        e.kind = ExprKind::Call {
            func: Box::new(ctor_path),
            args: vec![CallArg::Item(array_expr)],
            trailing: None,
        };
        true
    }

    fn walk_item(&mut self, item: &mut Item) {
        match item {
            Item::Fn(f) => match &mut f.body {
                FnBody::Expr(e) => self.walk_expr(e),
                FnBody::Block(b) => self.walk_block(b),
                FnBody::External => {}
            },
            Item::Const(c) => self.walk_expr(&mut c.value),
            Item::Let(l) => self.walk_expr(&mut l.value),
            Item::Test(t) => self.walk_block(&mut t.body),
            Item::Bench(b) => {
                for s in &mut b.setup {
                    self.walk_stmt(s);
                }
                self.walk_block(&mut b.measure_body);
                for s in &mut b.teardown {
                    self.walk_stmt(s);
                }
            }
            Item::Type(_) | Item::Lemma(_) => {}
        }
    }

    fn walk_block(&mut self, b: &mut Block) {
        for s in &mut b.stmts {
            self.walk_stmt(s);
        }
        if let Some(t) = &mut b.trailing {
            self.walk_expr(t);
        }
    }

    fn walk_stmt(&mut self, s: &mut Stmt) {
        match s {
            Stmt::Let(d) => self.walk_expr(&mut d.value),
            Stmt::Const(d) => self.walk_expr(&mut d.value),
            Stmt::Expr(e) => self.walk_expr(e),
            Stmt::Assign { target, value, .. } => {
                self.walk_expr(target);
                self.walk_expr(value);
            }
            Stmt::Return { value, .. } => {
                if let Some(v) = value {
                    self.walk_expr(v);
                }
            }
            Stmt::Throw { value, .. } => self.walk_expr(value),
            Stmt::Break(_) | Stmt::Continue(_) => {}
            Stmt::Defer { body, .. } => self.walk_expr(body),
            Stmt::ConsumeScope { init, body, .. } => {
                self.walk_expr(init);
                self.walk_block(body);
            }
            Stmt::AssertStatic { expr, .. } | Stmt::Assume { expr, .. } => self.walk_expr(expr),
            Stmt::Apply { .. } | Stmt::Calc { .. } | Stmt::Reveal { .. } => {}
            Stmt::TupleAssign { lhs, rhs, .. } => {
                for e in lhs {
                    self.walk_expr(e);
                }
                for e in rhs {
                    self.walk_expr(e);
                }
            }
        }
    }

    fn walk_expr(&mut self, e: &mut Expr) {
        if self.try_replace_embed(e) {
            return;
        }
        if self.try_replace_embed_dir(e) {
            return;
        }
        if self.try_replace_embed_str(e) {
            return;
        }
        match &mut e.kind {
            ExprKind::MapLit { elems, .. } => {
                for me in elems.iter_mut() {
                    match me {
                        MapElem::Pair(k, v) => {
                            self.walk_expr(k);
                            self.walk_expr(v);
                        }
                        MapElem::Spread(x) => self.walk_expr(x),
                    }
                }
            }
            ExprKind::ArrayLit(elems) => {
                for el in elems.iter_mut() {
                    match el {
                        ArrayElem::Item(x) | ArrayElem::Spread(x) => self.walk_expr(x),
                    }
                }
            }
            ExprKind::TupleLit(elems) => {
                for x in elems.iter_mut() {
                    self.walk_expr(x);
                }
            }
            ExprKind::RecordLit { fields, .. } => {
                for f in fields.iter_mut() {
                    if let Some(v) = &mut f.value {
                        self.walk_expr(v);
                    }
                }
            }
            ExprKind::Call { func, args, trailing } => {
                self.walk_expr(func);
                for a in args.iter_mut() {
                    match a {
                        CallArg::Item(x) | CallArg::Spread(x) => self.walk_expr(x),
                        CallArg::Named { value, .. } => self.walk_expr(value),
                    }
                }
                if let Some(t) = trailing {
                    match t {
                        Trailing::Block(b) => self.walk_block(b),
                        Trailing::LegacyBlockWithParams(tb) => self.walk_block(&mut tb.body),
                        Trailing::Fn(sb) => match &mut sb.body {
                            FnBody::Expr(x) => self.walk_expr(x),
                            FnBody::Block(b) => self.walk_block(b),
                            FnBody::External => {}
                        },
                    }
                }
            }
            ExprKind::TurboFish { base, .. } => self.walk_expr(base),
            ExprKind::Try(x) | ExprKind::Bang(x) | ExprKind::RefArg(x) => self.walk_expr(x),
            ExprKind::Coalesce(a, b) => {
                self.walk_expr(a);
                self.walk_expr(b);
            }
            ExprKind::As(x, _) | ExprKind::Is(x, _) => self.walk_expr(x),
            ExprKind::Binary { left, right, .. } => {
                self.walk_expr(left);
                self.walk_expr(right);
            }
            ExprKind::Unary { operand, .. } => self.walk_expr(operand),
            ExprKind::Member { obj, .. } => self.walk_expr(obj),
            ExprKind::Index { obj, index } => {
                self.walk_expr(obj);
                self.walk_expr(index);
            }
            ExprKind::If { cond, then, else_ } => {
                self.walk_expr(cond);
                self.walk_block(then);
                if let Some(eb) = else_ {
                    match eb {
                        ElseBranch::Block(b) => self.walk_block(b),
                        ElseBranch::If(x) => self.walk_expr(x),
                    }
                }
            }
            ExprKind::IfLet { scrutinee, guard, then, else_, .. } => {
                self.walk_expr(scrutinee);
                if let Some(g) = guard {
                    self.walk_expr(g);
                }
                self.walk_block(then);
                if let Some(eb) = else_ {
                    match eb {
                        ElseBranch::Block(b) => self.walk_block(b),
                        ElseBranch::If(x) => self.walk_expr(x),
                    }
                }
            }
            ExprKind::Match { scrutinee, arms } => {
                self.walk_expr(scrutinee);
                for arm in arms.iter_mut() {
                    if let Some(g) = &mut arm.guard {
                        self.walk_expr(g);
                    }
                    match &mut arm.body {
                        MatchArmBody::Expr(x) => self.walk_expr(x),
                        MatchArmBody::Block(b) => self.walk_block(b),
                    }
                }
            }
            ExprKind::For { iter, body, invariants, decreases, .. } => {
                self.walk_expr(iter);
                self.walk_block(body);
                for inv in invariants {
                    self.walk_expr(inv);
                }
                if let Some(d) = decreases {
                    self.walk_expr(d);
                }
            }
            ExprKind::ParallelFor { iter, body, .. } => {
                self.walk_expr(iter);
                self.walk_block(body);
            }
            ExprKind::While { cond, body, invariants, decreases } => {
                self.walk_expr(cond);
                self.walk_block(body);
                for inv in invariants {
                    self.walk_expr(inv);
                }
                if let Some(d) = decreases {
                    self.walk_expr(d);
                }
            }
            ExprKind::WhileLet { scrutinee, guard, body, .. } => {
                self.walk_expr(scrutinee);
                if let Some(g) = guard {
                    self.walk_expr(g);
                }
                self.walk_block(body);
            }
            ExprKind::Loop { body, .. } => self.walk_block(body),
            ExprKind::Block(b) => self.walk_block(b),
            ExprKind::Spawn(x) => self.walk_expr(x),
            ExprKind::Detach(b) | ExprKind::Blocking(b) => self.walk_block(b),
            ExprKind::Supervised { body, cancel, deadline, on_timeout } => {
                self.walk_block(body);
                if let Some(c) = cancel {
                    self.walk_expr(c);
                }
                if let Some(dl) = deadline {
                    self.walk_expr(&mut dl.expr);
                }
                if let Some(oh) = on_timeout {
                    self.walk_expr(oh);
                }
            }
            ExprKind::Forbid { body, .. } | ExprKind::Realtime { body, .. } => {
                self.walk_block(body);
            }
            ExprKind::Throw(x) => self.walk_expr(x),
            ExprKind::Interrupt(opt) => {
                if let Some(x) = opt {
                    self.walk_expr(x);
                }
            }
            // [E_COALESCE_RETURN_FALLBACK]: `X ?? return R` — checker-rejected
            // before this pass; walked defensively.
            ExprKind::CoalesceReturnFallback(opt) => {
                if let Some(x) = opt {
                    self.walk_expr(x);
                }
            }
            ExprKind::Range { start, end, .. } => {
                if let Some(s) = start {
                    self.walk_expr(s);
                }
                if let Some(x) = end {
                    self.walk_expr(x);
                }
            }
            ExprKind::InterpolatedStr { parts } => {
                for p in parts.iter_mut() {
                    if let InterpStrPart::Expr { expr: x, .. } = p {
                        self.walk_expr(x);
                    }
                }
            }
            ExprKind::TaggedTemplate { args, .. } => {
                for x in args.iter_mut() {
                    self.walk_expr(x);
                }
            }
            ExprKind::Lambda { body, .. } => self.walk_expr(body),
            ExprKind::ClosureLight { body, .. } => match body {
                ClosureBody::Expr(x) => self.walk_expr(x),
                ClosureBody::Block(b) => self.walk_block(b),
            },
            ExprKind::ClosureFull(sb) => match &mut sb.body {
                FnBody::Expr(x) => self.walk_expr(x),
                FnBody::Block(b) => self.walk_block(b),
                FnBody::External => {}
            },
            ExprKind::With { bindings, body } => {
                for b in bindings.iter_mut() {
                    self.walk_expr(&mut b.handler);
                }
                self.walk_block(body);
            }
            ExprKind::HandlerLit { methods, .. } | ExprKind::ProtocolLit { methods, .. } => {
                for m in methods.iter_mut() {
                    match &mut m.body {
                        HandlerMethodBody::Expr(x) => self.walk_expr(x),
                        HandlerMethodBody::Block(b) => self.walk_block(b),
                    }
                }
            }
            ExprKind::Select { arms } => {
                for arm in arms.iter_mut() {
                    match &mut arm.op {
                        SelectOp::Recv { chan, .. } => self.walk_expr(chan),
                        SelectOp::Send { chan, value } => {
                            self.walk_expr(chan);
                            self.walk_expr(value);
                        }
                        SelectOp::Default => {}
                    }
                    if let Some(g) = &mut arm.guard {
                        self.walk_expr(g);
                    }
                    self.walk_block(&mut arm.body);
                }
            }
            ExprKind::Forall { body, .. } | ExprKind::Exists { body, .. } => {
                self.walk_expr(body);
            }
            // Листовые — нет под-выражений.
            ExprKind::Ident(_)
            | ExprKind::Path(_)
            | ExprKind::SelfAccess
            | ExprKind::IntLit(_)
            | ExprKind::FloatLit(_)
            | ExprKind::BoolLit(_)
            | ExprKind::StrLit(_)
            | ExprKind::HexBlobLit(_)
            | ExprKind::CharLit(_)
            | ExprKind::UnitLit
            | ExprKind::NullPtrLit => {}
        }
    }
}

/// Plan 210 (D412-амендмент): рекурсивный обход `dir` (собственный обход,
/// НЕ follow symlinks — §2е/§4.2). `root` — исходный `embed_dir(...)`
/// корень (для вычисления POSIX-relative пути); `dir` — текущая
/// поддиректория обхода (== `root` на первом вызове).
///
/// - Dot-skip: имя записи, начинающееся с `.`, пропускается ЦЕЛИКОМ
///   (файл или папка) — правило касается записей ВНУТРИ обхода, не самого
///   `root` (тот уже канонизирован и принят вызывающим до этого обхода —
///   `embed_dir(".assets")` со явно названным dot-корнем встраивается).
///   Plan 210 Ф.7.3 (Go-паритет+): `hidden` (параметр, из `embed_dir(...,
///   hidden: true)`) ОТКЛЮЧАЕТ этот скип целиком для записей внутри обхода —
///   дефолт `false` = поведение не меняется.
/// - Symlink-skip: `DirEntry::metadata()` не следует symlink'ам (эквивалент
///   `symlink_metadata`) — файл/папка-симлинк пропускается, путь копится в
///   `symlinks_skipped` для `W_EMBED_DIR_SYMLINK_SKIPPED`. `hidden` НЕ влияет
///   на это правило — симлинки скипаются БЕЗУСЛОВНО (задание владельца
///   2026-07-17: hidden касается только dot-skip).
/// - Non-ASCII-skip НЕ означает пропуск встраивания — файл ВСТРАИВАЕТСЯ,
///   но его имя копится в `non_ascii` для `W_EMBED_DIR_NON_ASCII_PATH`
///   (непортируемый байтовый ключ, NFD/NFC — §2е).
/// - Каждый уровень сортируется по имени записи (`file_name`) перед
///   рекурсией — не для итогового порядка (тот пересортировывается по
///   полному POSIX-пути ПОСЛЕ обхода, см. `try_replace_embed_dir`), а для
///   детерминированного порядка НАКОПЛЕНИЯ `symlinks_skipped`/`non_ascii`
///   (порядок warning'ов тоже воспроизводим между прогонами/ОС).
fn walk_embed_dir_rec(
    root: &Path,
    dir: &Path,
    out: &mut Vec<(String, PathBuf)>,
    symlinks_skipped: &mut Vec<String>,
    non_ascii: &mut Vec<String>,
    hidden: bool,
) {
    let Ok(read_dir) = std::fs::read_dir(dir) else {
        return;
    };
    let mut children: Vec<std::fs::DirEntry> = read_dir.filter_map(|e| e.ok()).collect();
    children.sort_by_key(|e| e.file_name());
    for entry in children {
        let name_os = entry.file_name();
        let name = name_os.to_string_lossy().into_owned();
        // Dot-skip (§2е) — если НЕ отключён `hidden: true` (Ф.7.3).
        if !hidden && name.starts_with('.') {
            continue;
        }
        // Symlink-skip (§2е): DirEntry::metadata() не следует symlink'ам —
        // эквивалент symlink_metadata (Rust std contract), портируемо
        // между Windows/Unix. Безусловно — `hidden` этого не затрагивает.
        let Ok(meta) = entry.metadata() else { continue };
        if meta.file_type().is_symlink() {
            symlinks_skipped.push(entry.path().display().to_string());
            continue;
        }
        if !name.is_ascii() {
            non_ascii.push(name.clone());
        }
        let path = entry.path();
        if meta.is_dir() {
            walk_embed_dir_rec(root, &path, out, symlinks_skipped, non_ascii, hidden);
        } else if meta.is_file() {
            let rel = path.strip_prefix(root).unwrap_or(&path);
            // POSIX-путь: компоненты соединяются `/` независимо от ОС
            // (Windows `\`-разделитель обхода → `/` в ключе, §2д).
            let rel_posix = rel
                .components()
                .map(|c| c.as_os_str().to_string_lossy().into_owned())
                .collect::<Vec<_>>()
                .join("/");
            out.push((rel_posix, path));
        }
        // Прочие типы файлов (device/fifo/…) — вне объёма, тихо пропускаем.
    }
}
