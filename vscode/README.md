# RoseGold for Cursor

Language support for `.rg` files. Highlighting and snippets work on their own. Diagnostics, completion, hover, go to definition, references, rename, signature help, and format talk to the Rust `rg ide` language service.

## Install

Build `rg` (`cargo build -p rosegold-cli`) so `target/debug/rg` exists in the workspace, or set **RoseGold: Rg** (`rosegold.rg`) to the executable. The extension spawns `rg ide` and sends one JSON request per line.

## Commands

- **RoseGold: Run File** (`rg run`)
- **RoseGold: Test File** (`rg test`)
- **RoseGold: Restart Language Service**

Format Document uses the same formatter as `rg fmt`. Settings live under **RoseGold** (`rosegold.rg`, compact format, keep comments).
