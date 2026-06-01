---
name: release
description: Use when cutting a new release of gah - bumps the version, updates the changelog, tags, pushes, and creates the GitHub release. Triggers on "new release", "cut a release", "bump version", "release vX.Y.Z".
---

# Release - Cut a new gah release

End-to-end release workflow for this crate. Version lives in `Cargo.toml`;
`Cargo.lock` is gitignored (do not stage it). Releases are tagged `vX.Y.Z` and
published with `gh release`.

## 1. Recon

```bash
grep -m1 '^version' Cargo.toml          # current version
git tag --sort=-v:refname | head        # existing tags
gh release list -L 5                     # existing releases
gh auth status                           # confirm gh is logged in
git log <latest-tag>..HEAD --oneline     # commits to release
git diff <latest-tag>..HEAD --stat       # blast radius
```

## 2. Decide the bump (SemVer)

Read the commits since the last tag and classify the highest-impact change:

- **Breaking change** (`:boom:`, removed/renamed flag, changed output contract) → **major**.
- **New backward-compatible feature** (`:sparkles:`, new flag) → **minor**.
- **Bug fix / internal change only** (`:bug:`, `:recycle:`, docs, chore) → **patch**.

Docs-only and CI-only commits do not warrant a release on their own. When the
bump is ambiguous (e.g. a fix that changes behavior), ask the user.

## 3. Bump the version

Edit `Cargo.toml` `version = "X.Y.Z"`, then sync the lockfile:

```bash
cargo update -p gah --precise X.Y.Z
```

`Cargo.lock` is gitignored here, so the sync is for local build correctness
only — it will not be committed.

## 4. Update CHANGELOG.md

Follows [Keep a Changelog](https://keepachangelog.com/). Add a new section at
the top, under the header, dated with today's date (YYYY-MM-DD):

```markdown
## [X.Y.Z] - YYYY-MM-DD

### Added | Changed | Fixed | Removed

- User-facing description of the change and *why* it matters.
```

Also add the compare link at the bottom:

```markdown
[X.Y.Z]: https://github.com/ThatXliner/gah/compare/v<prev>...vX.Y.Z
```

Group entries under `Added` / `Changed` / `Deprecated` / `Removed` / `Fixed` /
`Security`. Describe behavior, not commit hashes.

## 5. Verify the build

```bash
cargo build        # must compile at the new version
cargo test         # must stay green
```

Do not tag a release that does not build.

## 6. Commit

Stage only `Cargo.toml` and `CHANGELOG.md` (never `Cargo.lock` — gitignored):

```bash
git add Cargo.toml CHANGELOG.md
git commit -m ":bookmark: chore(release): vX.Y.Z" -m "<one-line why>"
```

`:bookmark:` is the gitmoji for release/version-bump commits.

## 7. Tag and push

```bash
git tag -a vX.Y.Z -m "vX.Y.Z - <short title>"
git push origin main
git push origin vX.Y.Z
```

## 8. Create the GitHub release

Use the changelog section as the body. Keep the title `vX.Y.Z - <short title>`
consistent with prior releases.

```bash
gh release create vX.Y.Z \
  --title "vX.Y.Z - <short title>" \
  --notes "<changelog section body>

**Full Changelog**: https://github.com/ThatXliner/gah/compare/v<prev>...vX.Y.Z"
```

Confirm: `gh release view vX.Y.Z --json url -q .url`

## 9. Publish to crates.io

`gah` is a published crate — a release is not done until it is on crates.io.
This step is easy to forget; the tag and GitHub release do NOT publish the crate.

First confirm what will actually ship, then publish:

```bash
cargo package --list          # exactly which files go into the crate
cargo publish --dry-run       # package + compile + verify, no upload
cargo publish                 # the real upload
```

Verify it landed:

```bash
curl -s https://crates.io/api/v1/crates/gah \
  | python3 -c "import sys,json; print(json.load(sys.stdin)['crate']['max_version'])"
```

**`cargo publish` is irreversible.** A version can never be replaced or deleted,
only **yanked** (`cargo yank --version X.Y.Z`, undo with `--undo`). Yank stops
new dependents from selecting it; it does not remove the uploaded artifact. So:

- Run `--dry-run` and eyeball `cargo package --list` BEFORE the real publish.
- Confirm the version with the user before uploading.
- If a bad artifact ships (e.g. bloated, wrong files), you cannot fix it in
  place — yank it and publish a new patch version with the fix.

### Keep the crate lean

`cargo package --list` must contain only source: `src/`, `tests/`, `Cargo.toml`,
`README.md`, `CHANGELOG.md`, `Cargo.lock`. Demo assets, skills, plugin metadata,
CI, and docs are excluded via `exclude = [...]` in `Cargo.toml` — if any of those
reappear in the list, extend `exclude` before publishing.

## Notes

- Pushing tags, the GitHub release, and `cargo publish` are all outward-facing
  and effectively irreversible — confirm the version and notes with the user
  before steps 7 and 9 unless they have already approved the release.
- This repo gitignores `Cargo.lock`; on a project that tracks it, also stage the
  lockfile in step 6.
