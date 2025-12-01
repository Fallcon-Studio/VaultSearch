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

### Download a release archive (recommended)
1. Grab the latest archive for your platform from the Releases section (naming pattern: `vaultsearch-<VERSION>-<TARGET>.tar.gz` for Unix-like systems, `vaultsearch-<VERSION>-<TARGET>.zip` for Windows).
2. Extract it somewhere on your `PATH` (or copy the binary to a directory on your `PATH`).
   ```bash
   tar -xzf vaultsearch-<VERSION>-x86_64-unknown-linux-gnu.tar.gz
   sudo mv vaultsearch-<VERSION>-x86_64-unknown-linux-gnu/vaultsearch /usr/local/bin/
   vaultsearch --version
   ```
3. On Windows, unzip the archive and run the binary from PowerShell so you can see its output:
   ```powershell
   Expand-Archive -Path vaultsearch-<VERSION>-x86_64-pc-windows-gnu.zip -DestinationPath .
   .\vaultsearch-<VERSION>-x86_64-pc-windows-gnu\vaultsearch.exe --help
   ```

### Build from source
```bash
# From the repository root
cargo build --release
# The binary will be at target/release/vaultsearch (or target/release/vaultsearch.exe on Windows)
```

### Upgrading
- Download the newer release archive, replace your existing `vaultsearch` binary with the new one, and rerun `vaultsearch --version` to confirm the upgrade.
- Your configuration and index data live under your OS-specific config/data directories, so replacing the binary does not delete or reset your existing index. If a release changes the indexing format, rerun `vaultsearch index` after upgrading.

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

## Release artifacts and reproducible builds
- Run `scripts/package-release.sh` from the repository root to produce platform-specific archives under `dist/`. The script pins dependencies via `Cargo.lock` (`--locked`) and reuses the shared `target/` directory so repeated runs emit consistent outputs.
- Set the `TARGETS` environment variable to customize the build matrix (default targets: `x86_64-unknown-linux-gnu x86_64-pc-windows-gnu aarch64-apple-darwin`).
- Each archive contains the `vaultsearch` binary alongside `README.md` and `CHANGELOG.md` so consumers can keep documentation in sync with the shipped version.
- Prerequisites: the Rust toolchain (with `rustup` for target installation) plus `tar` and `zip` for packaging.

## Release notes and versioning
- VaultSearch follows semantic versioning; the current release is **0.2.0** (see `Cargo.toml`).
- Each release is described in `CHANGELOG.md`. When publishing a new version, update the changelog entry and increment the crate version so customers can match downloaded binaries to documented changes.

## Development
- Format code with `cargo fmt`.
- Run checks with `cargo test`.

## Notes
This project aims to stay offline-first. Keep your index on trusted storage and run searches locally to maintain privacy.
