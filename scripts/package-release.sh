#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DIST_DIR="$ROOT_DIR/dist"
TARGETS=${TARGETS:-"x86_64-unknown-linux-gnu x86_64-pc-windows-gnu aarch64-apple-darwin"}

VERSION=$(cargo metadata --no-deps --format-version 1 \
  | jq -r '.packages[] | select(.name=="vaultsearch").version' \
  | head -n1)

if [[ -z "$VERSION" ]]; then
  echo "Failed to read package version from cargo metadata" >&2
  exit 1
fi

echo "Building VaultSearch ${VERSION} for targets: ${TARGETS}" >&2
rm -rf "$DIST_DIR"
mkdir -p "$DIST_DIR"

for target in $TARGETS; do
  rustup target add "$target" >/dev/null

  echo "\n==> Compiling for ${target}" >&2
  CARGO_TARGET_DIR="$ROOT_DIR/target" cargo build --release --locked --target "$target"

  artifact_root="$DIST_DIR/vaultsearch-${VERSION}-${target}"
  mkdir -p "$artifact_root"

  bin_name="vaultsearch"
  [[ "$target" == *"windows"* ]] && bin_name="vaultsearch.exe"

  cp "$ROOT_DIR/target/${target}/release/${bin_name}" "$artifact_root/"
  cp "$ROOT_DIR/README.md" "$ROOT_DIR/CHANGELOG.md" "$artifact_root/"

  pushd "$DIST_DIR" >/dev/null
  if [[ "$target" == *"windows"* ]]; then
    archive_name="$(basename "$artifact_root").zip"
    zip -rq "$archive_name" "$(basename "$artifact_root")"
  else
    archive_name="$(basename "$artifact_root").tar.gz"
    tar -czf "$archive_name" "$(basename "$artifact_root")"
  fi

  sha256sum "$archive_name" >"$archive_name.sha256"

  if [[ "${SIGN_ARTIFACTS:-0}" == "1" ]]; then
    gpg_args=(--detach-sign --armor)
    [[ -n "${GPG_SIGNING_KEY:-}" ]] && gpg_args+=(--local-user "$GPG_SIGNING_KEY")
    gpg "${gpg_args[@]}" "$archive_name"
  fi
  popd >/dev/null

  rm -rf "$artifact_root"
done

echo "\nArtifacts ready under ${DIST_DIR}" >&2
