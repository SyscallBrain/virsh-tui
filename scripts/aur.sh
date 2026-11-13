#!/usr/bin/env bash
# Prepares the AUR packages virsh-tui (built from source) and virsh-tui-bin (the
# static release binaries) for a published release.
#
#   scripts/aur.sh VERSION AUR_DIR
#
# For each package, AUR_DIR/NAME is the package's AUR git repository; it is
# cloned (ssh://aur@aur.archlinux.org/NAME.git) when missing. The PKGBUILD
# from packaging/aur/NAME is copied there with the version and the checksums
# of the GitHub release, and .SRCINFO is regenerated. Nothing is committed or
# pushed: check the result with makepkg, then commit and push yourself
# (docs/RELEASE.md).
#
#   scripts/aur.sh VERSION AUR_DIR --local
#
# Tries the packages before the release exists: the source tarball comes from
# `git archive HEAD` and the binaries from dist/ (scripts/dist.sh), and AUR_DIR
# is a scratch directory, never cloned. Then run `makepkg` in each package.
set -euo pipefail
cd "$(dirname "$0")/.."

usage() { echo "usage: scripts/aur.sh VERSION AUR_DIR [--local]" >&2; exit 2; }
[ $# -ge 2 ] || usage
version=$1
aur_dir=$2
local_mode=false
[ "${3:-}" = "--local" ] && local_mode=true
[ $# -le 3 ] || usage

repo=https://github.com/SyscallBrain/virsh-tui
targets=(x86_64 aarch64)
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

# Checksums: the source tarball, then one per release binary.
source_tarball="virsh-tui-$version.tar.gz"
if $local_mode; then
  git archive --format=tar.gz --prefix="virsh-tui-$version/" -o "$work/$source_tarball" HEAD
  for target in "${targets[@]}"; do
    file="virsh-tui-$version-$target-unknown-linux-musl.tar.gz"
    if [ -f "dist/$file" ]; then
      cp "dist/$file" "$work/"
    else
      echo "note: dist/$file is missing; its checksum stays SKIP" >&2
    fi
  done
  (cd "$work" && sha256sum -- *.tar.gz > SHA256SUMS)
else
  curl -fsSL -o "$work/$source_tarball" "$repo/archive/v$version.tar.gz"
  curl -fsSL -o "$work/SHA256SUMS" "$repo/releases/download/v$version/SHA256SUMS"
  (cd "$work" && sha256sum -- "$source_tarball" >> SHA256SUMS)
fi

checksum() {
  local sum
  sum=$(awk -v f="$1" '$2 == f || $2 == "*"f { print $1 }' "$work/SHA256SUMS")
  echo "${sum:-SKIP}"
}

prepare() {
  local name=$1 dest="$aur_dir/$1"
  if [ ! -d "$dest" ]; then
    if $local_mode; then
      mkdir -p "$dest"
    else
      git clone "ssh://aur@aur.archlinux.org/$name.git" "$dest"
      # A new package is an empty repository; the AUR only accepts master,
      # whatever init.defaultBranch says.
      git -C "$dest" symbolic-ref HEAD refs/heads/master
    fi
  fi
  sed -e "s/^pkgver=.*/pkgver=$version/" "packaging/aur/$name/PKGBUILD" > "$dest/PKGBUILD"
  printf '%s\n' '*' '!PKGBUILD' '!.SRCINFO' '!.gitignore' > "$dest/.gitignore"
}

prepare virsh-tui
sed -i "s/^sha256sums=.*/sha256sums=('$(checksum "$source_tarball")')/" "$aur_dir/virsh-tui/PKGBUILD"

prepare virsh-tui-bin
for target in "${targets[@]}"; do
  sum=$(checksum "virsh-tui-$version-$target-unknown-linux-musl.tar.gz")
  sed -i "s/^sha256sums_$target=.*/sha256sums_$target=('$sum')/" "$aur_dir/virsh-tui-bin/PKGBUILD"
done

if $local_mode; then
  # makepkg uses a source file found next to the PKGBUILD instead of
  # downloading it.
  cp "$work/$source_tarball" "$aur_dir/virsh-tui/"
  cp "$work"/virsh-tui-"$version"-*-unknown-linux-musl.tar.gz "$aur_dir/virsh-tui-bin/" 2>/dev/null || true
fi

for name in virsh-tui virsh-tui-bin; do
  (cd "$aur_dir/$name" && makepkg --printsrcinfo > .SRCINFO)
  if grep -q "sha256sums.*SKIP" "$aur_dir/$name/PKGBUILD"; then
    echo "warning: $name still has a SKIP checksum" >&2
  fi
done

echo "Prepared $aur_dir/virsh-tui and $aur_dir/virsh-tui-bin for $version."
