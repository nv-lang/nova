# Nova Language — Zed Extension

Zed editor поддержка Nova: **tree-sitter syntax highlighting + LSP diagnostics**.

→ Полная инструкция установки: **[editors/INSTALL.md](../INSTALL.md)**

## Что даёт расширение

| Feature | Источник |
|---|---|
| Syntax highlighting | tree-sitter-nova v0.1.0 |
| LSP diagnostics | nova-lsp via stdio JSON-RPC |
| Auto-pairs `'` и `` ` `` | config.toml brackets |
| Block comments | `/* */` |

## Side-load установка (dev)

```bash
# Linux/macOS
mkdir -p ~/.config/zed/extensions/nova
cp -r editors/zed/* ~/.config/zed/extensions/nova/

# Windows
New-Item -ItemType Directory -Force "$env:APPDATA\Zed\extensions\nova"
Copy-Item -Recurse editors\zed\* "$env:APPDATA\Zed\extensions\nova\"
```

Потом перезапусти Zed. Extension появится в Extensions panel.

## Конфигурация nova-lsp binary path

В `settings.json` Zed:

```json
{
    "lsp": {
        "nova-lsp": {
            "binary": {
                "path": "/home/me/.cargo/bin/nova-lsp",
                "arguments": []
            }
        }
    }
}
```

Если не указано — ищет `nova-lsp` в PATH.

## Marketplace

Submission в Zed marketplace — deferred (ручной review process).
Tracking: [M-104.8-zed-marketplace] в simplifications.md.

## Структура

```
editors/zed/
├── extension.toml          # манифест (id, grammar reference, language_servers)
├── languages/
│   └── nova/
│       ├── config.toml     # language config (brackets, file-types, comments)
│       └── highlights.scm  # tree-sitter highlights (from tree-sitter-nova v0.1.0)
└── README.md               # этот файл
```

## Примечания

- `schema_version = 1` — PIN этот номер, Zed API меняется.
- `highlights.scm` — копия из `tree-sitter-nova/queries/highlights.scm`.
  При обновлении tree-sitter-nova → обновлять вручную.
- Grammar commit: `99111569f13e318d9f7de7f2c64bf2ef6185ba7a` (v0.1.0)
