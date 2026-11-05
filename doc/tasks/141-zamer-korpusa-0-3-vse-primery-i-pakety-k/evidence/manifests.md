# Manifests контура 141 (все хэши sha256)

| Артефакт | sha256 | Происхождение |
|---|---|---|
| oracle nova.exe (nova-cli release, HEAD) | `6876c9d11a1bebd2ad460538eb814801d4cee088224122cd22a5213a98136388` | `cargo build --release` в nova-cli/ worktree 141 |
| novac141.exe (Карина HEAD, oracle-built) | `d0ab736a04e59dbf66e10ebae17a73ef481a0909b7e2d57110ca29970c3f6460` | `nova build novac/src/main.nv` оракулом, 54.3 с |
| сид nova-carina-0.3.0-141head-windows-x86_64.zip | `f2647039711838bb4b8854d7621f42e47303d5a105cce89400daa81243f23832` | упакован локально из novac141.exe + SOURCE-SHA; НЕ опубликован |
| scratch-драйвер nova.exe (patched-scratch-driver-141) | `c47412b6d8749876fad90a0ef122f011c71c74ace10084afc64c03f1d53ff0fb` | копия nova-driver @ 0b01c838b + scratch-driver-141.patch |

## Дескриптор сида (только scratch-install141; официальный docs/dev/carina-seeds не тронут)

```json
{
  "asset_name": "nova-carina-0.3.0-141head-windows-x86_64.zip",
  "asset_url": "https://github.com/nv-lang/nova/releases/download/carina-v0.3.0-141head-local/nova-carina-0.3.0-141head-windows-x86_64.zip",
  "sha256": "f2647039711838bb4b8854d7621f42e47303d5a105cce89400daa81243f23832",
  "platform": "windows-x86_64",
  "source_revision": "0b01c838b57f4056000eeb46987252e91dcdad71",
  "compiler_provenance": "oracle:0b01c838b57f4056000eeb46987252e91dcdad71",
  "stage": "oracle-a"
}
```

`verify-seed` на каждом прогоне: PASS с NOTICE (stage oracle-a требует
`--allow-oracle-stage-a` + `--oracle-reason`; текст причины — в каждом логе
сборки, `logs-118/*.txt` / `logs-packages/*.txt`).
