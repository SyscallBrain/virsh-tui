#!/usr/bin/env bash
# Builds dist/virsh-tui-VERSION-1-ARCH.pkg.tar.zst from the static archive made
# by scripts/dist.sh, with makepkg. Runs makepkg directly on Arch Linux and in
# an archlinux:base-devel container elsewhere. ARCH may differ from the host:
# the package only copies the prebuilt binary.
set -euo pipefail
cd "$(dirname "$0")/.."

version=$1
arch=$2
tarball="virsh-tui-$version-$arch-unknown-linux-musl.tar.gz"
build=$(mktemp -d)
trap 'rm -rf "$build"' EXIT

checksum=$(sha256sum "dist/$tarball" | cut -d' ' -f1)
sed -e "s/@VERSION@/$version/" -e "s/@TARBALL@/$tarball/" -e "s/@SHA256@/$checksum/" \
  packaging/arch/PKGBUILD > "$build/PKGBUILD"
cp "dist/$tarball" "$build/"
# makepkg takes the architecture from its configuration, not the PKGBUILD,
# so append an override to the system configuration.
write_conf() { cat /etc/makepkg.conf; echo "CARCH=\"$arch\""; }

# makepkg refuses to run as root, so root uses the container path too.
if command -v makepkg >/dev/null && [ "$(id -u)" -ne 0 ]; then
  write_conf > "$build/makepkg.conf"
  (cd "$build" && makepkg --config makepkg.conf --clean --nodeps --force)
else
  docker run --rm -v "$build:/build" -e ARCH="$arch" -e HOST_UID="$(id -u)" -e HOST_GID="$(id -g)" \
    archlinux:base-devel bash -euc '
      { cat /etc/makepkg.conf; echo "CARCH=\"$ARCH\""; } > /build/makepkg.conf
      useradd --create-home builder
      chown -R builder /build
      su builder -c "cd /build && makepkg --config makepkg.conf --clean --nodeps --force"
      chown -R "$HOST_UID:$HOST_GID" /build'
fi
cp "$build"/virsh-tui-"$version"-1-"$arch".pkg.tar.zst dist/
