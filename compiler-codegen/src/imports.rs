//! Plan 35 Ф.1 MVP: cross-file import resolution через inline AST expansion.
//!
//! Используется тремя compile pipelines (Plan 35 R31 — unified pipeline):
//! - `nova-cli::cmd_check` — type-check single file.
//! - `nova-cli::cmd_build` — compile single file → exe.
//! - `compiler-codegen::test_runner::codegen_to_c` — test compilation.
//!
//! Все три вызывают `resolve_imports_inline(...)` ДО передачи `Module` в
//! `types::check_module` или `CEmitter::emit_module`.

use crate::ast::{Import, Item, Module, PeerFile};
use crate::diag::{byte_to_line_col, FileId, MAIN_FILE_ID};
use crate::parser;
use anyhow::{anyhow, Result};
use std::collections::HashSet;
use std::path::{Path, PathBuf};

/// Plan 35 Ф.1 MVP: cross-file resolve через inline AST expansion.
///
/// Walks `module.imports` recursively (BFS), loads each imported `.nv` file,
/// parses, recursively resolves transitive imports. `Item::Type`,
/// `Item::Fn`, `Item::Const` из всех imported modules merge'ятся в текущий
/// `module.items`.
///
/// **Cycle detection:** visited set по canonical path. Cycle → error.
///
/// **Load paths** (в порядке поиска):
///   1. `<entry_dir>/<path/parts>.nv` — same-package import
///   2. `<repo>/<path/parts>.nv`     — repo-root import (для `std.X.Y` это `<repo>/std/X/Y.nv`)
///   3. `<stdlib_dir>/<X/Y>.nv`      — explicit stdlib (если path начинается с `std.`)
///
/// **Limitations** (sub-plans 35.A-E):
///   - Нет visibility filter (is_export informational).
///   - Нет symbol mangling.
///   - Нет DCE.
///   - Нет signature/body 2-pass split.
///   - Wildcard `import X.*` не поддерживается.
pub fn resolve_imports_inline(
    entry_path: &Path,
    module: &mut Module,
    repo: &Path,
    stdlib_dir: &Path,
) -> Result<()> {
    resolve_imports_inline_ex(entry_path, module, repo, stdlib_dir, false)
}

/// Plan 42 правило F: `_test.nv` peers test-only.
/// `include_test_peers=true` (test mode): включает `*_test.nv` файлы
/// в folder-module collection.
/// `include_test_peers=false` (build mode): фильтрует их.
pub fn resolve_imports_inline_ex(
    entry_path: &Path,
    module: &mut Module,
    repo: &Path,
    stdlib_dir: &Path,
    include_test_peers: bool,
) -> Result<()> {
    let entry_dir = entry_path.parent().unwrap_or(repo).to_path_buf();
    // Plan 42.14 Ф.3 ([M11]): cycle detection keyed by declared module
    // name (Vec<String>), не canonical PathBuf — symlink-safe.
    let mut visited: HashSet<Vec<String>> = HashSet::new();

    let mut merged_items: Vec<Item> = Vec::new();

    // Plan 42 Sub-plan 42.4 шаг 2 (2026-05-14): per-peer attribution.
    // Entry's PeerFile регистрируется первым (file_id = MAIN_FILE_ID = 0).
    // imports + items_here — копия entry's pre-merge state.
    //
    // Note: entry parsed parent caller'ом через `parser::parse(src)` который
    // использует MAIN_FILE_ID, так что entry's spans уже file_id=0. Сейчас
    // лишь регистрируем PeerFile для type-checker'а.
    let entry_canon_for_peer = entry_path.canonicalize().unwrap_or_else(|_| entry_path.to_path_buf());
    let entry_peer_file = PeerFile {
        path: entry_canon_for_peer,
        file_id: MAIN_FILE_ID,
        imports: module.imports.clone(),
        items_here: module.items.clone(),
        // Plan 42.15: заполнится после resolve entry's imports.
        imported_item_names: HashSet::new(),
        // Plan 42.15: entry — часть компилируемого module.
        is_entry_module: true,
    };
    // Local counter для file_id (entry = 0, peers начинают с 1).
    // Используем Vec<PeerFile> чтобы collect peers через resolve_one,
    // потом append в module.peer_files после всех resolves.
    let mut peer_files: Vec<PeerFile> = vec![entry_peer_file];
    let mut next_file_id: FileId = 1;
    // Plan 35 Ф.1 cycle detection (D29) + Plan 42.14 Ф.3 ([M11]):
    // in-progress DFS-stack — declared module names (Vec<String>)
    // currently being resolved. Если import упирается в module name
    // уже в стеке → cycle. visited — closed-set (diamond-dep dedup);
    // in_progress — open-set (cycle detect).
    let mut in_progress: HashSet<Vec<String>> = HashSet::new();
    let mut import_chain: Vec<Vec<String>> = Vec::new(); // для error message

    // Plan 35 Ф.1 (D29): добавляем entry в in_progress + chain ДО resolve.
    // Если transitive import ссылается обратно на entry — cycle detected.
    // Entry key = его declared module name (module.name).
    let entry_key = module.name.clone();
    in_progress.insert(entry_key.clone());
    import_chain.push(module.name.clone());

    // Plan 35 sub-plan 35.A R27: auto-import `std.prelude` if exists.
    // D26 (08-runtime.md): prelude — auto-available имена (Option/Result/...).
    // Currently большая часть prelude hardcoded в type-checker'е/codegen'е;
    // R27 даёт миграционный путь — пользователь может расширять prelude
    // через `std/prelude.nv` файл (или future полная миграция hardcoded
    // в file-based). MVP: если файл существует — добавляем как import.
    // Skip prelude auto-import для самого prelude (избежать self-cycle).
    // Plan 42 Sub-plan 42.6: detect prelude self по обоих declaration
    // форматов (rev-1 legacy + rev-3 parent.X). Logic — в manifest helper.
    let is_prelude_self = crate::manifest::is_prelude_self_module(&module.name);
    let mut initial_imports = module.imports.clone();
    if !is_prelude_self {
        let prelude_path = stdlib_dir.join("prelude.nv");
        if prelude_path.exists() && prelude_path.is_file() {
            initial_imports.push(Import {
                path: vec!["std".into(), "prelude".into()],
                items: None,
                alias: None,
                is_export: false,
                span: crate::diag::Span::dummy(),
            });
        }
    }

    // Plan 42.10: accumulate module-level attrs from `_module.nv` peers
    // of imported folder-modules. Applied to entry's module.attrs at end.
    let mut inherited_attrs: Vec<crate::ast::ModuleAttr> = Vec::new();

    // Plan 42.15: accumulator visible-имён для entry's PeerFile.
    let mut entry_visible: HashSet<String> = HashSet::new();

    for imp in &initial_imports {
        resolve_one(
            imp,
            entry_path,
            &entry_dir,
            repo,
            stdlib_dir,
            &mut visited,
            &mut in_progress,
            &mut import_chain,
            &mut merged_items,
            &mut peer_files,
            &mut next_file_id,
            include_test_peers,
            &mut inherited_attrs,
            &mut entry_visible,
        )?;
    }

    // Plan 42.15: записываем visible-имена в entry's PeerFile (file_id=0).
    if let Some(entry_pf) = peer_files.iter_mut().find(|p| p.file_id == MAIN_FILE_ID) {
        entry_pf.imported_item_names = entry_visible;
    }

    // Entry done — promote из in_progress → visited.
    in_progress.remove(&entry_key);
    visited.insert(entry_key);
    import_chain.pop();

    // Prepend merged items: imported сначала, потом user code.
    // Это важно для bootstrap single-pass codegen — typedef'ы должны
    // появиться ДО use-site.
    let mut new_items = merged_items;
    new_items.append(&mut module.items);
    module.items = new_items;

    // Plan 42 Sub-plan 42.4 шаг 2: переносим собранные PeerFile в module.
    // Type-checker (шаг 3) использует это для per-peer name resolution.
    module.peer_files = peer_files;

    // Plan 42.10: merge inherited attrs из `_module.nv` peers импортированных
    // folder-modules. CapabilityCtx (types/mod.rs) применит #forbid attrs
    // ко всем functions module — независимо от того, defined ли они в
    // entry или imported. Doc и Cfg attrs тоже пропагируются (consumer —
    // Plan 45 nova doc и cfg_active filter уже handled).
    for attr in inherited_attrs {
        module.attrs.push(attr);
    }

    Ok(())
}

/// Plan 35 Ф.1 cycle detection (D29): DFS-recursive resolve.
/// Поддерживает два множества:
///   - `visited`: closed-set (модули уже полностью обработаны) — для
///     diamond-dep dedup (silent skip).
///   - `in_progress`: open-set (модули currently being resolved в DFS-стеке)
///     — для cycle detection (error при повторном visit'е).
///   - `import_chain`: parallel vec для error-message (full cycle path).
fn resolve_one(
    imp: &Import,
    // Plan 42.17 Ф.4: путь importing-файла (entry или peer, который
    // написал этот `import`). Нужен для Rule H filesystem-containment.
    importer_path: &Path,
    entry_dir: &Path,
    repo: &Path,
    stdlib_dir: &Path,
    visited: &mut HashSet<Vec<String>>,
    in_progress: &mut HashSet<Vec<String>>,
    import_chain: &mut Vec<Vec<String>>,
    merged_items: &mut Vec<Item>,
    peer_files: &mut Vec<PeerFile>,
    next_file_id: &mut FileId,
    include_test_peers: bool,
    // Plan 42.10: collect module-level attrs from `_module.nv` peers
    // for propagation into entry's module.attrs.
    inherited_attrs: &mut Vec<crate::ast::ModuleAttr>,
    // Plan 42.15: accumulator имён items, ставших видимыми через ЭТОТ
    // import (после rename). Caller — владелец import'а (peer/entry) —
    // передаёт свой `imported_item_names`. Транзитивные sub-imports
    // получают свой временный acc (не протекают в caller).
    visible_acc: &mut HashSet<String>,
) -> Result<()> {
    // Plan 42 правило H (`internal/` boundary) — проверяется НИЖЕ, после
    // resolve в filesystem paths. Plan 42.17 Ф.4 перевёл его с хрупкого
    // import-path-string prefix на filesystem-containment: re-export /
    // alias больше не обходят boundary.

    // Plan 42 Ф.2: resolve module to list of peer files (or single file
    // for legacy single-file modules).
    let resolved_paths = resolve_module_paths(&imp.path, entry_dir, repo, stdlib_dir, include_test_peers)
        .map_err(|err| {
            // Plan 42 правило L: diagnostic quality. Plan 42.08 Ф.2: ambiguous
            // case теперь явно диагностируется.
            let importing = import_chain
                .last()
                .map(|m| m.join("."))
                .unwrap_or_else(|| "<unknown>".to_string());
            match err {
                ResolveErr::Ambiguous { file, folder } => anyhow!(
                    "ambiguous module '{}': both single-file and folder-module exist\n  \
                     imported from: module `{}`\n  \
                     file:   {}\n  \
                     folder: {}\n  \
                     hint: remove one or rename to resolve conflict (D29 rev-3)",
                    imp.path.join("."),
                    importing,
                    file.display(),
                    folder.display(),
                ),
                ResolveErr::NotFound => {
                    let suggestion = suggest_module_name(
                        &imp.path,
                        entry_dir,
                        repo,
                        stdlib_dir,
                    );
                    anyhow!(
                        "cannot find module '{}'\n  \
                         imported from: module `{}`\n  \
                         searched:\n  \
                         \x20  {} (single-file or folder)\n  \
                         \x20  {} (single-file or folder)\n  \
                         \x20  {} (stdlib){}",
                        imp.path.join("."),
                        importing,
                        entry_dir.join(imp.path.iter().collect::<PathBuf>()).display(),
                        repo.join(imp.path.iter().collect::<PathBuf>()).display(),
                        if imp.path[0] == "std" && imp.path.len() >= 2 {
                            stdlib_dir.join(imp.path[1..].iter().collect::<PathBuf>())
                                .display()
                                .to_string()
                        } else {
                            "<n/a>".to_string()
                        },
                        suggestion,
                    )
                }
            }
        })?;

    // Plan 42 правило H + Plan 42.17 Ф.4: `internal/` boundary —
    // **filesystem-containment** check. `<owner>/internal/...` импортируем
    // ТОЛЬКО из файлов физически под `<owner>/`. Проверяем по реальному
    // пути importing-файла (`importer_path`) против реального пути
    // resolved `internal/`-модуля — не по строке import-path. Re-export
    // (`export import`) и alias обойти boundary не могут: проверяется
    // фактическое расположение файлов, а не путь, по которому дошли.
    if let Some(owner_dir) = find_internal_owner_dir(&resolved_paths[0]) {
        let importer_canon = importer_path.canonicalize()
            .unwrap_or_else(|_| importer_path.to_path_buf());
        let owner_canon = owner_dir.canonicalize()
            .unwrap_or_else(|_| owner_dir.clone());
        if !importer_canon.starts_with(&owner_canon) {
            let importing = import_chain.last()
                .map(|m| m.join("."))
                .unwrap_or_else(|| "<entry>".to_string());
            return Err(anyhow!(
                "cannot import internal module '{}' from outside its owner\n  \
                 internal module:  {}\n  \
                 owner directory:  {}\n  \
                 importing file:   {}\n  \
                 importing module: {}\n  \
                 hint: `internal/` modules are accessible only from files \
                 under `{}` (Plan 42 rule H)",
                imp.path.join("."),
                resolved_paths[0].display(),
                owner_canon.display(),
                importer_canon.display(),
                importing,
                owner_canon.display(),
            ));
        }
    }

    // Plan 42.14 Ф.3 ([M11]): cycle detection по DECLARED MODULE NAME,
    // не canonical PathBuf. Symlink / case-insensitive FS могли дать
    // разные пути для same module → false-negative cycle. Module name
    // (parent.X) — стабильный логический identity.
    //
    // Читаем `module X.Y` declaration из первого peer (lightweight —
    // только первая non-comment строка, без полного parse).
    let first_path = &resolved_paths[0];
    let module_key: Vec<String> = read_module_decl(first_path)
        .unwrap_or_else(|| {
            // Fallback: если decl не прочитался — canonical path string
            // как single-element key (всё равно уникален).
            let canon = first_path.canonicalize()
                .unwrap_or_else(|_| first_path.clone());
            vec![canon.to_string_lossy().to_string()]
        });

    // D29: cycle = module_key уже в in_progress.
    if in_progress.contains(&module_key) {
        let mut chain_display: Vec<String> = import_chain.iter()
            .map(|p| p.join("."))
            .collect();
        chain_display.push(imp.path.join("."));
        return Err(anyhow!(
            "import cycle detected:\n  {}",
            chain_display.join(" → ")));
    }

    // Closed-set: diamond-dep dedup. Silent skip.
    if visited.contains(&module_key) {
        return Ok(());
    }

    in_progress.insert(module_key.clone());
    import_chain.push(imp.path.clone());

    // Plan 42 Ф.2: parse все peer files в alphabetical order (правило B).
    // Для each peer:
    //   1. Parse to Module.
    //   2. Recursively resolve its imports.
    //   3. Append its items в merged_items.
    // Peers share namespace через merge'нутый Module.items.
    for peer_path in &resolved_paths {
        let peer_canon = peer_path.canonicalize()
            .map_err(|e| anyhow!("canonicalize {}: {}", peer_path.display(), e))?;

        let peer_src = std::fs::read_to_string(peer_path)
            .map_err(|e| anyhow!("failed to read imported module {}: {}", peer_path.display(), e))?;
        let peer_path_str = peer_path.to_string_lossy().to_string();

        // Plan 42 Sub-plan 42.4 шаг 2: allocate unique FileId для этого peer
        // и parse с этим file_id. Все tokens/spans peer'а получат этот id,
        // type-checker (шаг 3) использует для per-peer name resolution.
        let peer_file_id = *next_file_id;
        *next_file_id += 1;

        let peer_module = parser::parse_with_file_id(&peer_src, peer_file_id)
            .map_err(|d| {
                let (line, col) = byte_to_line_col(&peer_src, d.span.start);
                anyhow!(
                    "in imported module '{}' ({}): {}:{}: {}",
                    imp.path.join("."), peer_path_str, line, col, d.message)
            })?;

        // Plan 42.12 Ф.2: проверка module-level `#cfg(feature/target_os)`.
        // Если peer объявил inactive cfg — skip целиком (не merge items,
        // не register peer_file, не recurse imports).
        if !cfg_active(&peer_module) {
            continue;
        }

        // Plan 42.10: `_module.nv` peer — special module-config файл.
        // Его module-level attrs (Forbid / Cfg / Doc) пропагируются на
        // entry's module.attrs — applied ко всему compiled module.
        let is_module_config = peer_path.file_stem()
            .and_then(|s| s.to_str())
            .map_or(false, |stem| stem == "_module");
        if is_module_config {
            for attr in &peer_module.attrs {
                inherited_attrs.push(attr.clone());
            }
        }

        // Регистрируем PeerFile (snapshot до recursive resolve + merge).
        // Plan 42.15: imported_item_names заполняется ниже после resolve.
        // is_entry_module = false — это peer ИМПОРТИРОВАННОГО модуля,
        // его items_here НЕ должны протекать в entry's shared_decls.
        peer_files.push(PeerFile {
            path: peer_canon,
            file_id: peer_file_id,
            imports: peer_module.imports.clone(),
            items_here: peer_module.items.clone(),
            imported_item_names: HashSet::new(),
            is_entry_module: false,
        });

        // Plan 42.15: accumulator имён items видимых ЭТОМУ peer'у через
        // его прямые imports. Передаётся в resolve_one для каждого sub —
        // resolve_one пишет туда имена items которые sub притащил.
        let mut peer_visible: HashSet<String> = HashSet::new();

        // Recursive: resolve transitive imports for THIS peer.
        for sub in &peer_module.imports {
            // Plan 42.15: re-export. Если peer делает `export import X`
            // (sub.is_export) — items притащенные `sub` re-export'ятся:
            // они видны не только этому peer'у, но и caller'у (тому кто
            // импортировал ЭТОТ folder-module). Собираем в отдельный acc
            // и потом мержим в caller's visible_acc если is_export.
            let mut sub_visible: HashSet<String> = HashSet::new();
            resolve_one(
                sub,
                peer_path,
                entry_dir,
                repo,
                stdlib_dir,
                visited,
                in_progress,
                import_chain,
                merged_items,
                peer_files,
                next_file_id,
                include_test_peers,
                inherited_attrs,
                &mut sub_visible,
            )?;
            // Items всегда видны самому peer'у.
            for n in &sub_visible {
                peer_visible.insert(n.clone());
            }
            // `export import` — re-export: items видны caller'у, НО через
            // селективный фильтр самого caller'а (Plan 42.17 Ф.6): если
            // caller написал `import F.{a}` — он получает только `a` из
            // re-export'ов F, не другие re-exported items.
            // Note: rename caller'а к re-exported items НЕ применяется —
            // re-exported item уже в merged_items под именем re-export'а,
            // переименовать его здесь без рассинхрона с codegen-scope
            // нельзя. Rename работает для прямых (не re-exported) imports.
            if sub.is_export {
                for n in &sub_visible {
                    if import_selects(imp, n) {
                        visible_acc.insert(n.clone());
                    }
                }
            }
        }

        // Plan 42.15: записываем собранные visible-имена в PeerFile.
        // Находим PeerFile по file_id (он был push'нут выше; recursive
        // resolve_one мог push'нуть ещё peer_files, ищем по id).
        if let Some(pf) = peer_files.iter_mut().find(|p| p.file_id == peer_file_id) {
            pf.imported_item_names = peer_visible;
        }

        // Plan 42.09: selective rename map. Если import имеет
        // `.{A as B}` — после merge item с name `A` переименовывается
        // в `B` в merged scope.
        let rename_map: std::collections::HashMap<String, String> =
            if let Some(items) = &imp.items {
                items.iter()
                    .filter_map(|it| it.alias.as_ref().map(|a| (it.name.clone(), a.clone())))
                    .collect()
            } else {
                std::collections::HashMap::new()
            };
        // Merge items from this peer (with optional rename).
        // Plan 42.15: имена merged items пишутся в `visible_acc` —
        // caller (peer/entry который написал `imp`) получает их в свой
        // visible scope. Это и есть «import притащил эти имена».
        for item in peer_module.items {
            let name = match &item {
                Item::Type(t) => Some(t.name.clone()),
                Item::Fn(f) => Some(f.name.clone()),
                Item::Const(c) => Some(c.name.clone()),
                Item::Test(_) | Item::Let(_) => None,
            };
            match (&item, name) {
                (Item::Type(_) | Item::Fn(_) | Item::Const(_), Some(item_name)) => {
                    // Codegen completeness: ВСЕ items merge'атся в
                    // merged_items (inline expansion — `A`'s body может
                    // ссылаться на `B` из того же модуля). Selective list
                    // влияет на visibility, не на codegen-scope.
                    let final_name = if let Some(new_name) = rename_map.get(&item_name) {
                        let renamed = rename_item(item, new_name.clone());
                        merged_items.push(renamed);
                        new_name.clone()
                    } else {
                        merged_items.push(item);
                        item_name.clone()
                    };
                    // Plan 42.15: visible_acc — что caller's peer ВИДИТ.
                    // Selective filter: `import X.{A}` — только `A` виден
                    // caller'у; `B` (тоже из X, не в списке) merge'ится в
                    // merged_items для codegen completeness, но НЕ виден
                    // caller'у по имени. Матч по оригинальному `item_name`;
                    // в scope кладётся `final_name` (renamed при alias).
                    if import_selects(imp, &item_name) {
                        visible_acc.insert(final_name);
                    }
                }
                _ => {
                    // Test blocks / top-level let — игнорируем для imported.
                }
            }
        }
    }

    // Plan 42.14 Ф.3: pop in_progress + chain; promote module_key в
    // closed-set. Все peers folder-module share один module_key (declared
    // name) — diamond-dep dedup работает естественно.
    in_progress.remove(&module_key);
    visited.insert(module_key);
    import_chain.pop();
    Ok(())
}

/// Plan 42.17 Ф.6: видит ли селективный список `imp` имя `name`?
/// `import X` (без `.{...}`) — видит всё. `import X.{a, b}` — только
/// `a`/`b`. Матч по ОРИГИНАЛЬНОМУ имени item'а; `alias` — это что
/// кладётся в scope (`final_name`), не критерий отбора.
fn import_selects(imp: &Import, name: &str) -> bool {
    match &imp.items {
        None => true,
        Some(sel) => sel.iter().any(|it| it.name == name),
    }
}

/// Plan 42.17 Ф.4: если `path` лежит внутри `.../<owner>/internal/...`,
/// возвращает `.../<owner>/` — owner-директорию для Rule H containment
/// check. None если `internal` сегмента в пути нет.
///
/// Spec D29 rev-3.1: «берётся **первый** internal сегмент» — поэтому при
/// nested `internal/` берём самый ВЕРХНИЙ. `internal` на самом верху
/// (нет родителя) → None.
fn find_internal_owner_dir(path: &Path) -> Option<PathBuf> {
    let mut cur = path;
    let mut internal_dir: Option<&Path> = None;
    while let Some(parent) = cur.parent() {
        if parent.file_name().map(|n| n == "internal").unwrap_or(false) {
            // Перезаписываем — итоговое значение = самый верхний `internal`.
            internal_dir = Some(parent);
        }
        cur = parent;
    }
    internal_dir.and_then(|d| d.parent()).map(|p| p.to_path_buf())
}

/// Plan 42.17 Ф.3: единый сканер `module a.b` декларации из исходника —
/// заменяет три копипаст-сканера (`read_module_decl` + два folder-module
/// detector'а в `test_runner.rs`).
///
/// Lightweight: первая значимая строка, без полного parse. Пропускает
/// blank / `//` / `#`-attr строки (Plan 42.16 — module-level атрибуты
/// идут ПЕРЕД `module`). Nova не имеет block-комментариев (`/* */`) —
/// лексер обрабатывает только `//`, поэтому отдельная их обработка не
/// нужна. Первая non-skip строка не `module ...` → `None`.
///
/// Возвращает имя модуля как сегменты: `module encoding.hex` →
/// `["encoding", "hex"]`. Trailing-комментарий после декларации
/// отбрасывается (`module a.b // note` → `["a", "b"]`).
pub fn scan_module_decl(src: &str) -> Option<Vec<String>> {
    for raw in src.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with("//") || line.starts_with('#') {
            continue;
        }
        if let Some(rest) = line.strip_prefix("module ") {
            let decl = rest.trim().split_whitespace().next().unwrap_or("");
            if decl.is_empty() {
                return None;
            }
            return Some(decl.split('.').map(|s| s.to_string()).collect());
        }
        // Первая значимая строка не `module` — декларации нет.
        return None;
    }
    None
}

/// Plan 42.14 Ф.3 ([M11]): cycle-detection key — declared module name
/// (не canonical path). Тонкая обёртка над `scan_module_decl`.
fn read_module_decl(path: &Path) -> Option<Vec<String>> {
    let src = std::fs::read_to_string(path).ok()?;
    scan_module_decl(&src)
}

/// Plan 42.09: rename item (Type/Fn/Const) при selective re-import.
/// `import X.{A as B}` → A in module X становится B в importing module.
fn rename_item(item: Item, new_name: String) -> Item {
    match item {
        Item::Type(mut t) => {
            t.name = new_name;
            Item::Type(t)
        }
        Item::Fn(mut f) => {
            f.name = new_name;
            Item::Fn(f)
        }
        Item::Const(mut c) => {
            c.name = new_name;
            Item::Const(c)
        }
        other => other,
    }
}

/// Plan 42 правило L: suggest module name через scan parent dir.
/// Если в parent dir есть похожие .nv files или folders — предложить
/// «did you mean ...?». Возвращает «\n  hint: ...» string или empty.
fn suggest_module_name(
    parts: &[String],
    entry_dir: &Path,
    repo: &Path,
    _stdlib_dir: &Path,
) -> String {
    if parts.is_empty() {
        return String::new();
    }
    // Scan parent dir of expected path в entry_dir / repo.
    let target = parts.last().cloned().unwrap_or_default();
    let parent_parts = &parts[..parts.len() - 1];
    let parent_rel: PathBuf = parent_parts.iter().collect();
    let mut candidates: Vec<String> = Vec::new();
    for root in [entry_dir, repo] {
        let dir = root.join(&parent_rel);
        if let Ok(entries) = std::fs::read_dir(&dir) {
            for entry in entries.filter_map(|e| e.ok()) {
                let path = entry.path();
                if path.is_dir() {
                    if let Some(name) = path.file_name().and_then(|s| s.to_str()) {
                        candidates.push(name.to_string());
                    }
                } else if path.extension().and_then(|s| s.to_str()) == Some("nv") {
                    if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                        candidates.push(stem.to_string());
                    }
                }
            }
        }
    }
    // Cheap similar-name match: case-insensitive substring or prefix.
    let target_lower = target.to_lowercase();
    let close: Vec<String> = candidates
        .iter()
        .filter(|c| {
            let cl = c.to_lowercase();
            cl == target_lower || cl.starts_with(&target_lower) || target_lower.starts_with(&cl)
        })
        .cloned()
        .collect();
    if close.is_empty() {
        return String::new();
    }
    let suggestion = close
        .iter()
        .take(3)
        .map(|c| {
            let mut p = parent_parts.to_vec();
            p.push(c.clone());
            p.join(".")
        })
        .collect::<Vec<_>>()
        .join(", ");
    format!("\n  hint: did you mean `{}`?", suggestion)
}

/// Plan 42.12 Ф.2: enabled features set (через `NOVA_FEATURES=foo,bar` env
/// или `--features` CLI flag). Empty if нет features.
pub fn enabled_features() -> HashSet<String> {
    if let Ok(s) = std::env::var("NOVA_FEATURES") {
        s.split(',').map(|x| x.trim().to_string()).filter(|x| !x.is_empty()).collect()
    } else {
        HashSet::new()
    }
}

/// Plan 42.14 Ф.1: рекурсивная оценка одного `#cfg` predicate.
/// `any` — OR, `all` — AND, `not` — negation.
pub fn eval_cfg_predicate(
    pred: &crate::ast::CfgPredicate,
    target: &str,
    features: &HashSet<String>,
) -> bool {
    use crate::ast::CfgPredicate as P;
    match pred {
        P::Feature(name) => features.contains(name),
        P::TargetOs(os) => match os.as_str() {
            "windows" => target == "windows",
            "linux" => target == "linux",
            "macos" => target == "macos",
            "unix" | "posix" => target == "linux" || target == "macos" || target == "unix",
            _ => false, // unknown target = never matches
        },
        P::Any(preds) => preds.iter().any(|p| eval_cfg_predicate(p, target, features)),
        P::All(preds) => preds.iter().all(|p| eval_cfg_predicate(p, target, features)),
        P::Not(inner) => !eval_cfg_predicate(inner, target, features),
    }
}

/// Plan 42.12 Ф.2 + 42.14 Ф.1: peer module active при current target/features?
/// Проверяет все `#cfg` атрибуты — если хоть один inactive → peer inactive.
/// (AND semantic между разными `#cfg` атрибутами; внутри одного — `any/all/not`.)
fn cfg_active(module: &Module) -> bool {
    let target = current_target_os();
    let features = enabled_features();
    for attr in &module.attrs {
        if let crate::ast::ModuleAttrKind::Cfg(pred) = &attr.kind {
            if !eval_cfg_predicate(pred, target, &features) {
                return false;
            }
        }
    }
    true
}

/// Plan 42.12 Ф.1: target OS для filename suffix filtering.
/// Default — host OS (cfg!(target_os) at compile time of nova-codegen).
/// Override через `NOVA_TARGET_OS` env var (Ф.1 minimal — без CLI flag).
pub fn current_target_os() -> &'static str {
    // Override через env var — валидируем против известных значений и
    // возвращаем `&'static str` literal (без Box::leak: невалидное имя
    // никогда не матчится, "unknown" честнее утёкшей мусорной строки).
    if let Ok(t) = std::env::var("NOVA_TARGET_OS") {
        return match t.as_str() {
            "windows" => "windows",
            "linux" => "linux",
            "macos" => "macos",
            "unix" | "posix" => "unix",
            _ => "unknown",
        };
    }
    if cfg!(target_os = "windows") { "windows" }
    else if cfg!(target_os = "linux") { "linux" }
    else if cfg!(target_os = "macos") { "macos" }
    else if cfg!(target_family = "unix") { "unix" }
    else { "unknown" }
}

/// Plan 42.12 Ф.1: filename suffix filter для peer files.
/// Returns Some(target) если filename имеет recognized suffix (`_windows.nv`,
/// `_linux.nv`, `_macos.nv`, `_unix.nv`, `_posix.nv`); None если нет suffix.
fn file_target_suffix(stem: &str) -> Option<&'static str> {
    // Order matters: check more specific suffixes first.
    // `_test` тоже может быть в stem'е — мы фильтруем после _test stripping
    // в caller, так что здесь работаем с already-stripped stem.
    if stem.ends_with("_windows") { Some("windows") }
    else if stem.ends_with("_linux") { Some("linux") }
    else if stem.ends_with("_macos") { Some("macos") }
    else if stem.ends_with("_unix") { Some("unix") }
    else if stem.ends_with("_posix") { Some("posix") }
    else { None }
}

/// Public wrapper для test_runner walker.
pub fn peer_active_for_target_pub(stem: &str, target: &str) -> bool {
    peer_active_for_target(stem, target)
}

/// Plan 42.12 Ф.1: peer file active для current target?
/// - Без suffix → активен всегда.
/// - С suffix → активен если target matches:
///   - `_windows` ↔ windows
///   - `_linux` ↔ linux
///   - `_macos` ↔ macos
///   - `_unix` ↔ linux OR macos (POSIX-like, без bsd для simplicity)
///   - `_posix` ↔ linux OR macos (синоним _unix)
fn peer_active_for_target(stem: &str, target: &str) -> bool {
    match file_target_suffix(stem) {
        None => true,
        Some("windows") => target == "windows",
        Some("linux") => target == "linux",
        Some("macos") => target == "macos",
        Some("unix") | Some("posix") => target == "linux" || target == "macos" || target == "unix",
        Some(_) => true,
    }
}

/// Plan 42 Ф.2: resolve module to **list** of peer files (folder-module)
/// или single file. Returns `Vec<PathBuf>` alphabetically sorted (правило B).
///
/// Plan 42.08 Ф.2: возвращает `ResolveErr::Ambiguous` если `X.nv` И `X/`
/// (с direct .nv) сосуществуют — раньше silent None → generic "cannot find".
///
/// Plan 42.12 Ф.1: filter peer files по filename suffix vs current target.
///
/// Resolution order:
/// 1. Try single-file `<...>/parts.nv` (legacy behaviour).
/// 2. If not found, try folder `<...>/parts/` — collect все `*.nv` файлы
///    в этой папке (non-recursive, alphabetical sort).
/// 3. Conflict (file exists AND folder with .nv files exists) → `Err(Ambiguous)`.
///
/// Каждый search root (entry_dir / repo / stdlib_dir) проверяется в
/// порядке.
#[derive(Debug)]
pub(crate) enum ResolveErr {
    /// Не найдено — caller emit'ит «cannot find module» с suggestions.
    NotFound,
    /// `X.nv` и `X/` (с direct .nv) сосуществуют — ambiguous.
    Ambiguous { file: PathBuf, folder: PathBuf },
}

fn resolve_module_paths(
    parts: &[String],
    entry_dir: &Path,
    repo: &Path,
    stdlib_dir: &Path,
    include_test_peers: bool,
) -> Result<Vec<PathBuf>, ResolveErr> {
    if parts.is_empty() {
        return Err(ResolveErr::NotFound);
    }
    let rel_path: PathBuf = parts.iter().collect();

    // Candidate search roots — same order as resolve_import_path.
    let mut roots: Vec<PathBuf> = vec![
        entry_dir.to_path_buf(),
        repo.to_path_buf(),
    ];
    if parts[0] == "std" && parts.len() >= 2 {
        roots.push(stdlib_dir.to_path_buf());
    }

    for root in &roots {
        // Translate path: для stdlib_dir мы пропускаем первый `std` segment.
        let local_rel: PathBuf = if root == stdlib_dir && parts[0] == "std" {
            parts[1..].iter().collect()
        } else {
            rel_path.clone()
        };

        let single_file = root.join(local_rel.with_extension("nv"));
        let folder = root.join(&local_rel);

        let file_exists = single_file.is_file();
        let folder_exists = folder.is_dir();

        if file_exists && folder_exists {
            // Check folder has direct .nv files — only then it's ambiguous.
            // If folder только contains sub-folders without direct .nv,
            // we treat it as namespace-container (rule E).
            let has_direct_nv = std::fs::read_dir(&folder)
                .ok()
                .map(|entries| {
                    entries.filter_map(|e| e.ok()).any(|e| {
                        e.path().extension().and_then(|s| s.to_str()) == Some("nv")
                    })
                })
                .unwrap_or(false);
            if has_direct_nv {
                // Plan 42.08 Ф.2: ambiguous → return explicit ResolveErr
                // вместо silent None. Caller emit'ит clear «ambiguous module
                // X: <file> vs <folder>» вместо generic «cannot find».
                return Err(ResolveErr::Ambiguous {
                    file: single_file.clone(),
                    folder: folder.clone(),
                });
            }
        }

        if file_exists {
            return Ok(vec![single_file]);
        }

        if folder_exists {
            // Collect все *.nv files (non-recursive), alphabetical sort.
            // Plan 42 правило F: filter `*_test.nv` peers если
            // !include_test_peers (build mode).
            // Plan 42.12 Ф.1: filter peers по filename suffix vs current target.
            let target = current_target_os();
            let entries = match std::fs::read_dir(&folder) {
                Ok(e) => e,
                Err(_) => continue,
            };
            let mut peers: Vec<PathBuf> = entries
                .filter_map(|e| e.ok())
                .map(|e| e.path())
                .filter(|p| {
                    if !p.is_file() {
                        return false;
                    }
                    if p.extension().and_then(|s| s.to_str()) != Some("nv") {
                        return false;
                    }
                    if let Some(stem) = p.file_stem().and_then(|s| s.to_str()) {
                        // Strip `_test` suffix first для test-peer filter.
                        let core_stem = stem.strip_suffix("_test").unwrap_or(stem);
                        if !include_test_peers && core_stem != stem {
                            // `_test` peer, build mode → skip.
                            return false;
                        }
                        // Target filter: применяем к stem БЕЗ `_test` suffix'а
                        // (чтобы `tls_windows_test.nv` правильно ассоциировался
                        // с windows target).
                        if !peer_active_for_target(core_stem, target) {
                            return false;
                        }
                    }
                    true
                })
                .collect();
            if !peers.is_empty() {
                peers.sort();
                return Ok(peers);
            }
            // Folder без .nv files (после filter) — namespace-container,
            // не module. Продолжаем поиск в других roots.
        }
    }

    Err(ResolveErr::NotFound)
}
