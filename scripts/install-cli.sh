#!/usr/bin/env bash
set -euo pipefail

version="${TODOER_CLI_VERSION:-v1.0.0}"
if [[ ! "$version" =~ ^v(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)$ ]]; then
  echo "TODOER_CLI_VERSION must have the form vX.Y.Z" >&2
  exit 1
fi

case "$(uname -s):$(uname -m)" in
  Linux:x86_64|Linux:amd64) target=x86_64-unknown-linux-gnu ;;
  Linux:aarch64|Linux:arm64) target=aarch64-unknown-linux-gnu ;;
  Darwin:x86_64) target=x86_64-apple-darwin ;;
  Darwin:arm64) target=aarch64-apple-darwin ;;
  *) echo "Unsupported platform: $(uname -s) $(uname -m). See GitHub Releases for other downloads." >&2; exit 1 ;;
esac

command -v curl >/dev/null || { echo "curl is required" >&2; exit 1; }
if command -v sha256sum >/dev/null; then
  hash_command=sha256sum
elif command -v shasum >/dev/null; then
  hash_command=shasum
else
  echo "sha256sum or shasum is required" >&2
  exit 1
fi

asset="todoer-cli-${version}-${target}"
base="https://github.com/mattmezza/todoer.pro/releases/download/cli-${version}"
temp_dir="$(mktemp -d)"
trap 'rm -rf "$temp_dir"' EXIT
curl --fail --location --silent --show-error "$base/$asset" -o "$temp_dir/$asset"
curl --fail --location --silent --show-error "$base/$asset.sha256" -o "$temp_dir/$asset.sha256"
(
  cd "$temp_dir"
  if [[ "$hash_command" == sha256sum ]]; then
    sha256sum --check "$asset.sha256"
  else
    shasum -a 256 --check "$asset.sha256"
  fi
)

install_dir="${TODOER_INSTALL_DIR:-$HOME/.local/bin}"
mkdir -p "$install_dir"
# Copy into the destination filesystem before replacing an existing binary.
staged="$(mktemp "$install_dir/.todoer.XXXXXXXX")"
trap 'rm -rf "$temp_dir"; rm -f "$staged"' EXIT
cat "$temp_dir/$asset" > "$staged"
chmod 755 "$staged"
mv -f "$staged" "$install_dir/todoer"
echo "Installed Todoer CLI $version to $install_dir/todoer"
case ":$PATH:" in
  *":$install_dir:"*) ;;
  *) echo "Add $install_dir to your PATH to run todoer from any directory." ;;
esac
