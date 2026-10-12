#!/usr/bin/env bash
set -euo pipefail

project_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$project_root"
cargo build --release --locked -p cad-cli

platform="$(uname -s | tr '[:upper:]' '[:lower:]')-$(uname -m)"
package_name="headless-cad-$platform"
package_stage="$(mktemp -d "${TMPDIR:-/tmp}/headless-cad-package.XXXXXX")"
package_archive=""
cleanup() {
  rm -rf "$package_stage"
  if [[ -n "$package_archive" ]]; then
    rm -f "$package_archive"
  fi
}
trap cleanup EXIT
mkdir -p "$package_stage/$package_name" "$project_root/dist"
package_destination="$project_root/dist/$package_name.tar.gz"
if [[ -d "$package_destination" ]]; then
  printf 'Package destination is a directory: %s\n' "$package_destination" >&2
  exit 1
fi
package_archive="$(mktemp "$project_root/dist/.$package_name.XXXXXX.tar.gz")"
cp "${CARGO_TARGET_DIR:-$project_root/target}/release/cad" "$package_stage/$package_name/cad"
cp README.md LICENSE "$package_stage/$package_name/"
cp -R docs "$package_stage/$package_name/docs"
cp -R examples "$package_stage/$package_name/examples"
tar -czf "$package_archive" -C "$package_stage" "$package_name"
tar -tzf "$package_archive" >/dev/null
mv -f "$package_archive" "$package_destination"
printf '%s\n' "$package_destination"
