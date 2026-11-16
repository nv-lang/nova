#!/usr/bin/env bash
# SPDX-License-Identifier: MIT OR Apache-2.0
set -euo pipefail

version=$1
output=$(realpath "$2")
scratch=$(mktemp -d)
trap 'rm -rf -- "$scratch"' EXIT
name="nova-oracle-$version-linux-x86_64"
package="$scratch/$name"
mkdir -p "$package" "$output"
install /usr/local/bin/nova "$package/nova"
cp -a /opt/nova/{std,nova_rt,gc} "$package/"
find "$package" -type f -name .git -delete

cat > "$package/setup-env.sh" <<'SETUP'
#!/usr/bin/env bash
nova_root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
export PATH="$nova_root:$PATH"
export NOVA_STD_PATH="$nova_root/std"
export NOVA_CG_INCLUDE="$nova_root"
export NOVA_RT_DIR="$nova_root/nova_rt"
export NOVA_GC_LIB_DIR="$nova_root/gc/lib"
export NOVA_GC_INCLUDE_DIR="$nova_root/gc/include"
unset nova_root
SETUP

tar -C "$scratch" -czf "$output/$name.tar.gz" "$name"
mkdir "$scratch/install" "$scratch/project"
tar -C "$scratch/install" -xzf "$output/$name.tar.gz"
cd "$scratch/project"
cat > nova.toml <<'MANIFEST'
[package]
name = "hello"
version = "0.1.0"
MANIFEST
cat > hello.nv <<'NOVA'
module hello

fn main() Io {
    println("Hello, Nova!")
}
NOVA

export NOVA_CACHE=0
verify() {
    local nova=$1
    test "$("$nova" --version)" = "nova $version"
    "$nova" build hello.nv -o "$2"
    test "$("./$2")" = "Hello, Nova!"
    # The linked ELF contains the collector and its enlarged static-root limit.
    nm "$2" | awk '$3 == "GC_get_version" && $2 ~ /^[Tt]$/ {found=1} END {exit !found}'
    objdump -d --disassemble=GC_add_roots_inner "$2" |
        awk '/cmp.*\$0x2000,/ {found=1; print} END {exit !found}'
    if ldd "$2" | grep 'libgc\.so'; then
        echo "Unexpected dynamic Boehm GC dependency" >&2
        return 1
    fi
}
verify /usr/local/bin/nova hello-image
unset NOVA_STD_PATH NOVA_RT_DIR NOVA_CG_INCLUDE NOVA_GC_LIB_DIR NOVA_GC_INCLUDE_DIR
source "$scratch/install/$name/setup-env.sh"
verify nova hello-package
