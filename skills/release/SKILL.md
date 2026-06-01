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

## Notes

- Pushing tags and publishing a release are outward-facing and effectively
  irreversible — confirm the version and notes with the user before step 7
  unless they have already approved the release.
- This repo gitignores `Cargo.lock`; on a project that tracks it, also stage the
  lockfile in step 6.
