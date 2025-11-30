# VaultSearch

VaultSearch is a privacy-focused, offline search utility that builds a local Tantivy index over a folder of text-like files. It provides a simple command-line interface to initialize configuration, index your files, and search for matches without sending data anywhere else.

## Features
- Config-driven indexing rooted at a directory you choose.
- Fast full-text search powered by [Tantivy](https://tantivy-search.github.io/).
- Skips non-text file types to avoid noisy results.
- Clear progress reporting during indexing.

## Requirements
- Rust 1.70+ (edition 2021)
- A filesystem location that you can read and has enough space for the index.

## Installation
```bash
# From the repository root
cargo build --release
# The binary will be at target/release/vaultsearch (or vaultsearch.exe on Windows)
```

On Windows, run the tool from a terminal such as PowerShell rather than double-clicking the
executable so you can see its output. From the repository root after building, run:

```powershell
cd target\release
./vaultsearch.exe --help
./vaultsearch.exe init --root C:\\path\\to\\documents
# The binary will be at target/release/vaultsearch
```

## Usage
Run `vaultsearch --help` to see all options. The typical workflow looks like this:

1. **Initialize** configuration and index location
   ```bash
   vaultsearch init --root /path/to/documents
   ```
   This stores a `config.toml` alongside a Tantivy index in your platform's configuration/data directory (e.g., `~/.config/vaultsearch`).

2. **Index** the files under your root directory
   ```bash
   vaultsearch index
   ```
   Progress is printed in batches so you can monitor indexing throughput.

3. **Search** for terms
   ```bash
   vaultsearch search "invoice 2024"
   ```
   The top-ranked results (by score) are printed as JSON documents containing the file path and indexed contents.

## Configuration
The tool stores configuration and index data using your OS-specific directories (provided by the `directories` crate). On most systems you can find:
- `config.toml` under the user configuration directory (e.g., `~/.config/vaultsearch`).
- Index data under the user local data directory (e.g., `~/.local/share/vaultsearch/index`).

You can edit `config.toml` manually if you need to change the root or index location, or rerun `vaultsearch init` with a different `--root` to recreate it.

## Development
- Format code with `cargo fmt`.
- Run checks with `cargo test`.

## Notes
This project aims to stay offline-first. Keep your index on trusted storage and run searches locally to maintain privacy.
