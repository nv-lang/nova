#!/usr/bin/env python3
"""
Cat-B folder-module conversion script.

For each eligible dir:
1. Find positive .nv files (no EXPECT_COMPILE_ERROR, no EXPECT_RUNTIME_PANIC, no fn main)
2. Find conflicting top-level names (defined in >=2 files)
3. Rename conflicting names within each file: NAME -> NAME<N> (N=1,2,3... per file order)
4. Change module declarations: module X.stem -> module nova_tests.X
5. Move EXPECT_COMPILE_ERROR files to neg/ with module neg.stem

Run with --dry-run to preview, without to apply.
"""
import re
import sys
import shutil
from pathlib import Path

DRY_RUN = '--dry-run' in sys.argv
VERBOSE = '--verbose' in sys.argv or DRY_RUN

ROOT = Path(__file__).parent.parent / 'nova_tests'

# Names that look like stdlib types - skip renaming these (they're references, not local defs)
STDLIB_NAMES = {
    'Vec', 'Option', 'Result', 'str', 'int', 'bool', 'float',
    'char', 'u8', 'i8', 'u16', 'i16', 'u32', 'i32', 'u64', 'i64', 'f32', 'f64',
    'VecIter', 'MapIt', 'FiltIt', 'RangeIter', 'StepRangeIter', 'ReverseRangeIter',
    'Range', 'HashMap', 'HashSet', 'Io', 'Gc', 'Blocking',
}


def get_module_decl(src: str) -> str | None:
    m = re.search(r'^module\s+(\S+)', src, re.MULTILINE)
    return m.group(1) if m else None


def is_neg_file(src: str) -> bool:
    return 'EXPECT_COMPILE_ERROR' in src or 'EXPECT_RUNTIME_PANIC' in src


def has_fn_main(src: str) -> bool:
    return bool(re.search(r'^fn main\b', src, re.MULTILINE))


def get_top_level_names(src: str) -> list[str]:
    """Return all top-level defined names (fn/type/const/effect)."""
    return re.findall(r'^(?:fn|type|const|effect)\s+(\w+)', src, re.MULTILINE)


def real_blocked(d: Path) -> bool:
    """Hard blockers: nova.toml or non-neg sub-dirs with .nv files."""
    if (d / 'nova.toml').exists():
        return True
    for sd in d.iterdir():
        if sd.is_dir() and sd.name != 'neg' and list(sd.glob('*.nv')):
            return True
    return False


def rename_in_source(src: str, old_name: str, new_name: str) -> str:
    """Rename all word-boundary occurrences of old_name -> new_name in source."""
    return re.sub(rf'\b{re.escape(old_name)}\b', new_name, src)


def convert_dir(d: Path) -> dict:
    """
    Returns stats dict: {converted, neg_moved, conflicts_renamed, skipped_reason}
    """
    stats = {'dir': d.name, 'converted': 0, 'neg_moved': 0,
             'conflicts_renamed': 0, 'skipped_reason': None}

    if real_blocked(d):
        stats['skipped_reason'] = 'blocked (nova.toml or non-neg sub-dir)'
        return stats

    nv_files = sorted(d.glob('*.nv'))
    if not nv_files:
        stats['skipped_reason'] = 'no .nv files'
        return stats

    # Read all files
    file_data = []
    for f in nv_files:
        try:
            src = f.read_text(encoding='utf-8')
        except Exception as e:
            stats['skipped_reason'] = f'read error: {e}'
            return stats
        file_data.append((f, src))

    # Check already folder-module
    all_modules = set()
    for _, src in file_data:
        m = get_module_decl(src)
        if m:
            all_modules.add(m)
    if len(all_modules) == 1:
        stats['skipped_reason'] = 'already folder-module'
        return stats

    # Separate: neg files, standalone (fn main), positive
    neg_files = [(f, src) for f, src in file_data if is_neg_file(src)]
    standalone = [(f, src) for f, src in file_data if not is_neg_file(src) and has_fn_main(src)]
    pos_files = [(f, src) for f, src in file_data if not is_neg_file(src) and not has_fn_main(src)]

    if len(pos_files) < 2:
        stats['skipped_reason'] = f'only {len(pos_files)} positive non-main file(s)'
        return stats

    # Find conflicts among positives
    all_names: dict[str, list[int]] = {}  # name -> list of indices in pos_files
    for i, (f, src) in enumerate(pos_files):
        for name in get_top_level_names(src):
            all_names.setdefault(name, []).append(i)

    conflicts = {n: idxs for n, idxs in all_names.items()
                 if len(idxs) >= 2 and n not in STDLIB_NAMES}

    if VERBOSE and conflicts:
        print(f'  {d.name}: {len(conflicts)} conflicts: {list(conflicts.keys())[:6]}')

    # Assign rename index per file per conflicting name
    # For each conflict: sort file indices alphabetically by filename,
    # assign suffix 1,2,3...
    # Only rename in files where the name is DEFINED (top-level decl)
    renames: dict[int, dict[str, str]] = {}  # file_idx -> {old -> new}
    for name, idxs in conflicts.items():
        sorted_idxs = sorted(idxs, key=lambda i: pos_files[i][0].name)
        for rank, idx in enumerate(sorted_idxs, start=1):
            renames.setdefault(idx, {})[name] = f'{name}{rank}'

    # Apply renames + module change to positive files
    new_pos = []
    for i, (f, src) in enumerate(pos_files):
        new_src = src
        file_renames = renames.get(i, {})
        for old, new in file_renames.items():
            new_src = rename_in_source(new_src, old, new)
            stats['conflicts_renamed'] += 1
        # Change module decl
        new_src = re.sub(
            r'^(module\s+)\S+',
            f'module nova_tests.{d.name}',
            new_src,
            count=1,
            flags=re.MULTILINE
        )
        new_pos.append((f, new_src))

    # Handle standalone files (fn main): just change module decl, keep as-is
    # They stay in parent dir, test_runner treats them as standalone CUs
    new_standalone = []
    for f, src in standalone:
        new_src = re.sub(
            r'^(module\s+)\S+',
            f'module nova_tests.{d.name}',
            src, count=1, flags=re.MULTILINE
        )
        new_standalone.append((f, new_src))

    # Handle neg files: move to neg/ subdir
    neg_dir = d / 'neg'
    new_neg = []
    for f, src in neg_files:
        stem = f.stem
        new_src = re.sub(
            r'^(module\s+)\S+',
            f'module neg.{stem}',
            src, count=1, flags=re.MULTILINE
        )
        new_neg.append((f, neg_dir / f.name, new_src))

    # --- Apply changes ---
    if not DRY_RUN:
        # Write positive files (in place, renamed)
        for f, new_src in new_pos:
            f.write_text(new_src, encoding='utf-8')

        # Write standalone files (module decl only change)
        for f, new_src in new_standalone:
            f.write_text(new_src, encoding='utf-8')

        # Move neg files to neg/
        if new_neg:
            neg_dir.mkdir(exist_ok=True)
        for old_f, new_f, new_src in new_neg:
            new_f.write_text(new_src, encoding='utf-8')
            if old_f != new_f:
                old_f.unlink()

    stats['converted'] = len(pos_files)
    stats['neg_moved'] = len(new_neg)
    return stats


def find_eligible_dirs() -> list[Path]:
    """Find dirs that are Cat-B candidates (not already FM, >=2 pos files, not hard-blocked)."""
    result = []
    for d in sorted(ROOT.iterdir()):
        if not d.is_dir():
            continue
        if real_blocked(d):
            continue
        nv_files = list(d.glob('*.nv'))
        if len(nv_files) < 2:
            continue
        # Check not already FM
        modules = set()
        for f in nv_files:
            try:
                src = f.read_text(encoding='utf-8')
                m = get_module_decl(src)
                if m:
                    modules.add(m)
            except:
                pass
        if len(modules) == 1:
            continue  # already FM
        # Count positives
        pos = [f for f in nv_files
               if not is_neg_file(f.read_text(encoding='utf-8', errors='replace'))
               and not has_fn_main(f.read_text(encoding='utf-8', errors='replace'))]
        if len(pos) >= 2:
            result.append(d)
    return result


def main():
    target = sys.argv[1] if len(sys.argv) > 1 and not sys.argv[1].startswith('-') else None

    if target:
        dirs = [ROOT / target]
    else:
        dirs = find_eligible_dirs()

    print(f'Cat-B conversion: {len(dirs)} dirs {"(DRY RUN)" if DRY_RUN else ""}')
    print()

    total_converted = 0
    total_neg = 0
    total_renames = 0
    skipped = []

    for d in dirs:
        stats = convert_dir(d)
        if stats['skipped_reason']:
            skipped.append((d.name, stats['skipped_reason']))
            continue
        total_converted += stats['converted']
        total_neg += stats['neg_moved']
        total_renames += stats['conflicts_renamed']
        if VERBOSE:
            print(f'  OK {d.name}: {stats["converted"]} pos, '
                  f'{stats["neg_moved"]} neg moved, '
                  f'{stats["conflicts_renamed"]} renames')

    print(f'Converted: {len(dirs) - len(skipped)} dirs')
    print(f'  positive files updated: {total_converted}')
    print(f'  neg files moved:        {total_neg}')
    print(f'  conflict renames:       {total_renames}')
    if skipped:
        print(f'Skipped ({len(skipped)}):')
        for name, reason in skipped[:20]:
            print(f'  {name}: {reason}')
        if len(skipped) > 20:
            print(f'  ... and {len(skipped)-20} more')


if __name__ == '__main__':
    main()
