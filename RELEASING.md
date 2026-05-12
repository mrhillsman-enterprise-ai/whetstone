# Releasing Whetstone

This file describes:

1. the supported release process today
2. the safeguards built into it
3. the remaining improvements worth considering

## Short answer

The supported release path is now:

```text
just release patch|minor|major|set X.Y.Z
```

That command verifies the repo, bumps version files, creates a release branch,
pushes it, and opens a release PR.

After that PR is merged into `main`, GitHub Actions:

1. re-verifies format, clippy, and tests
2. builds release artifacts
3. packages bundled assets
4. creates the git tag
5. generates checksums
6. creates the GitHub release
7. verifies the GitHub release metadata
8. publishes the crate to crates.io

## Supported commands

### `just release-check`

Release verification gate:

```text
cargo fmt --check
cargo test
cargo clippy -- -D warnings
```

Use this when you want the local release gate without actually cutting a
release PR.

### `just release <bump>`

The supported release entrypoint.

Examples:

```text
just release patch
just release minor
just release set 2.2.0
```

This command depends on `release-check`, then runs:

```text
cargo run -- release <bump>
```

### `just release-publish <bump>`

Deprecated legacy path.

It is still present only so old habits fail loudly with guidance instead of
silently doing the wrong thing. It no longer performs a release.

## Current release flow

## 1. Start from a correct local state

`whetstone release` now enforces these preconditions before it edits version
files:

- working tree must be clean
- current branch must be `main`
- local `main` must match `origin/main`
- `gh` must be installed and authenticated
- target release branch and tag must not already exist locally or remotely

This is implemented in `src/release.rs`.

## 2. Run the release command

Example:

```text
just release patch
```

That performs the local verification gate, then `whetstone release patch`:

1. reads `VERSION`
2. computes the next version
3. updates `VERSION`
4. syncs `Cargo.toml`
5. creates branch `release/vX.Y.Z`
6. commits `release: vX.Y.Z`
7. pushes the branch
8. opens a PR to `main`

The PR body explicitly says that merging triggers the verified release
workflow.

## 3. Merge the release PR

Merging the PR is the release boundary.

There is now one intended automation path:

```text
release PR -> merge -> verified release workflow -> publish
```

## 4. Release workflow on `main`

`.github/workflows/release.yml` runs only when `VERSION` changes on `main`.

It now has these stages:

### `check-tag`

- reads `VERSION`
- computes `vX.Y.Z`
- enters recovery mode if the tag already exists

### `verify`

Runs the shared release gate before any tag or publish step:

- installs `just`
- runs `just release-check`

This means publish no longer races ahead of verification.

### `build`

Builds binaries for:

- `x86_64-unknown-linux-gnu`
- `aarch64-unknown-linux-gnu`
- `x86_64-apple-darwin`
- `aarch64-apple-darwin`

### `assets`

Packages `assets/` as `whetstone-assets.tar.gz`.

### `release`

- creates and pushes the tag
- downloads artifacts
- generates `SHA256SUMS.txt`
- creates the GitHub release

### `verify-release`

Checks that the GitHub release:

- exists for the expected tag
- is not draft
- is not prerelease
- contains the expected target archives, assets tarball, and `SHA256SUMS.txt`
- includes checksum entries for every published `.tar.gz` asset

### `publish-crate`

Only runs after the verification and GitHub release checks succeed.

It publishes the crate with:

```text
cargo publish --allow-dirty
```

using `CARGO_REGISTRY_TOKEN` from GitHub secrets.

## Why this is safer than before

The old flow had multiple competing release paths and a release workflow that
could publish without doing its own verification first.

The current flow reduces that risk by:

- making `just release ...` the only supported path
- turning `release-publish` into a deprecated failure path
- enforcing clean-tree and branch preconditions in Rust
- adding `release-check` locally
- reusing the shared `just release-check` gate in CI and release workflow
- generating checksums automatically
- verifying the GitHub release before publish

## Things to watch during a release

Even with the automation in place, a maintainer should still watch:

1. the release PR
2. CI on that PR
3. the release workflow after merge
4. the GitHub release page
5. crates.io publish completion

## Contributor checklist

Use this checklist for normal releases:

1. checkout `main`
2. pull latest
3. run `just release patch|minor|major|set X.Y.Z`
4. review the generated release PR
5. merge the PR
6. watch the release workflow through publish

If you only want to verify release readiness locally, run:

```text
just release-check
```

## Remaining improvements worth considering

The release process is in much better shape now, but these are still worth
considering later:

### 1. Crates.io post-publish smoke check

A small verification job could poll crates.io for the new version before the
workflow reports success. I did not add this yet because crates.io propagation
can be slightly delayed and make the workflow flaky.

### 2. Artifact signing

Checksums are now generated, but cryptographic signing would be stronger.

### 3. Windows release artifacts

The workflow currently builds Linux and macOS artifacts only. That matches the
previous behavior, but if Windows binaries become important, add them here.

### 4. Release notes curation

The workflow currently uses autogenerated GitHub release notes. If release
notes need more structure, add a changelog or a release notes template.

## Files involved in releases

- `justfile`
- `src/release.rs`
- `.github/workflows/release.yml`
- `.github/workflows/ci.yml`
- `VERSION`
- `Cargo.toml`

## Deprecated behavior

Do not use `just release-publish ...` as a normal release flow.

It is intentionally deprecated so this repo has one supported automated release
path instead of several half-overlapping ones.
