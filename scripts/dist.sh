#!/usr/bin/env bash
# Builds the release files for one target in dist/:
#   virsh-tui-VERSION-TARGET.tar.gz          static binary for any Linux
#   virsh-tui_VERSION-1_ARCH.deb             Debian, Ubuntu
#   virsh-tui-VERSION-1.ARCH.rpm             Fedora, openSUSE, RHEL
#   virsh-tui-VERSION-1-ARCH.pkg.tar.zst     Arch Linux
#
#   scripts/dist.sh x86_64-unknown-linux-musl
#   scripts/dist.sh aarch64-unknown-linux-musl --package-only
#
# --package-only skips the build and packages target/TARGET/release/virsh-tui,
# for a binary built on another machine. Building needs a musl C compiler
# for mimalloc (musl-gcc: Debian/Ubuntu musl-tools, Arch musl). Packaging
# needs cargo-deb, cargo-generate-rpm, and makepkg or Docker for the Arch
# package.
set -euo pipefail
cd "$(dirname "$0")/.."

target=${1:?usage: scripts/dist.sh TARGET [--package-only]}
version=$(cargo metadata --no-deps --format-version 1 |
  python3 -c 'import json,sys; print(next(p["version"] for p in json.load(sys.stdin)["packages"] if p["name"] == "virsh-tui"))')
arch=${target%%-*}

if [ "${2:-}" != "--package-only" ]; then
  cargo build --release --locked --target "$target"
fi
binary="target/$target/release/virsh-tui"
[ -x "$binary" ] || { echo "missing $binary" >&2; exit 1; }

mkdir -p dist
package="virsh-tui-$version-$target"
staging=$(mktemp -d)
trap 'rm -rf "$staging"' EXIT
mkdir "$staging/$package"
cp "$binary" LICENSE README.md CHANGELOG.md "$staging/$package/"
tar -C "$staging" -czf "dist/$package.tar.gz" --owner=0 --group=0 --mode=go-w "$package"

# The binary is already stripped by the release profile; the host strip may
# not understand a foreign architecture.
cargo deb --no-build --no-strip --target "$target" --output dist/
cargo generate-rpm --target "$target" --output dist/
scripts/arch-package.sh "$version" "$arch"

ls -l dist
