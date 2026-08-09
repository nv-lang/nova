# p259: окружение для замеров. Рантайм/GC берём из ГЛАВНОЙ репы (копия
# рантайма в worktree запрещена стражем check-no-runtime-copy.sh, №138).
export NOVA_GC_LIB_DIR="D:/Sources/nv-lang/nova/compiler-codegen/vcpkg_installed/x64-windows-static/lib"
export NOVA_GC_INCLUDE_DIR="D:/Sources/nv-lang/nova/compiler-codegen/vcpkg_installed/x64-windows-static/include"
export NOVA_INCLUDE_DIR="D:/Sources/nv-lang/nova/compiler-codegen/vcpkg_installed/x64-windows-static/include"
export NOVA_CG_INCLUDE="D:/Sources/nv-lang/nova/compiler-codegen"
export NOVA_RT_DIR="D:/Sources/nv-lang/nova/compiler-codegen/nova_rt"
export NOVA="D:/Sources/nv-lang/nova-p259/.p259/nova.exe"
unset NOVA_STD_PATH 2>/dev/null || true
