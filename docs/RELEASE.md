# Release checklist

1. The gates are green: `cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test`.
2. `CHANGELOG.md` has the release section `## [X.Y.Z] - YYYY-MM-DD`; the
   Release workflow uses it as the release notes and fails without it.
3. The new version is in `Cargo.toml` (and `Cargo.lock`), in
   `packaging/aur/*/PKGBUILD` and in the file names of the README and
   `book/src/installation.md`.
4. The book (`book/src/`) matches the behaviour; screenshots
   (`scripts/screenshots.py`) and the presentation GIF
   (`scripts/presentation.py`) are regenerated if the UI changed.
5. Tag `vX.Y.Z` and push the tag. The Release workflow builds static (musl)
   binaries for x86_64 and aarch64, the `.tar.gz`, `.deb`, `.rpm` and Arch
   `.pkg.tar.zst` files and `SHA256SUMS`, installs each x86_64 package on
   Debian, Ubuntu, Fedora, openSUSE and Arch, and publishes the GitHub
   release. Running the workflow by hand (Actions, Release, Run workflow)
   does everything except publish; the files are in the `dist` artifact.
   `scripts/dist.sh TARGET` builds the same files locally.
6. Publish the AUR packages (below) once the GitHub release exists.

## AUR

Two packages: `virsh-tui` builds from the tag's source tarball,
`virsh-tui-bin` installs the static binaries of the release. Their PKGBUILDs
live in `packaging/aur/`; `scripts/aur.sh` turns them into AUR-ready
repositories with the release's checksums and `.SRCINFO`.

### Once per machine

1. Add your SSH public key (`~/.ssh/id_ed25519.pub`) in your AUR account:
   <https://aur.archlinux.org>, My Account, SSH Public Key.
2. Tell SSH to use it for the AUR, in `~/.ssh/config`:

   ```text
   Host aur.archlinux.org
     IdentityFile ~/.ssh/id_ed25519
     User aur
   ```

3. Check: `ssh aur@aur.archlinux.org help` lists the AUR commands.

### For each release

```sh
scripts/aur.sh X.Y.Z ~/aur        # clones ~/aur/virsh-tui and ~/aur/virsh-tui-bin if needed
cd ~/aur/virsh-tui-bin && makepkg -si && namcap PKGBUILD ./*.pkg.tar.zst
cd ~/aur/virsh-tui     && makepkg -si && namcap PKGBUILD ./*.pkg.tar.zst
```

`namcap` warns that `libvirt` may not be needed: virsh-tui runs `virsh` as a
program instead of linking to libvirt, so the dependency is real. If both
packages install and run, publish each one:

```sh
git add PKGBUILD .SRCINFO .gitignore
git commit -m "Update to X.Y.Z"
git push
```

The first push to a name creates the package; the AUR only accepts the
`master` branch (the script sets it). For a packaging fix without a new
release, raise `pkgrel` in the AUR repository's PKGBUILD, regenerate
`.SRCINFO` with `makepkg --printsrcinfo > .SRCINFO`, and copy the change back
to `packaging/aur/`. `scripts/aur.sh X.Y.Z /tmp/aur --local` tries both
packages before a release, from `git archive HEAD` and `dist/`.
