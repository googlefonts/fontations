# Releasing

We use [`cargo-release`] to help guide the release process. It can be installed
with `cargo install cargo-release`. You may need to install `pkg-config` via your
package manager for this to work.

## Determining Versions and Crates to Release

1. **Find modified crates**: Run `cargo release changes` (or inspect `git log <crate>-v<version>..HEAD -- <crate>/`) to see which crates have been modified since their last release tag.
   * Even for a **non-breaking (`patch`) bump**, if a downstream crate being released depends on new APIs added to an upstream crate since its last release, the upstream crate must also be bumped and released so the workspace dependency requirement in the root `Cargo.toml` resolves to a published version containing those APIs.
2. **Choose the version bump (`minor` vs. `patch`)**:
   * All workspace crates are currently pre-`1.0` (`0.x.y`).
   * **Breaking changes** require a **minor** version bump (`0.x.0`).
   * **Non-breaking changes** (bug fixes, internal changes, or purely additive APIs) only require a **patch** version bump (`0.x.y`).
3. **Check for cascading releases across workspace dependencies**:
   * Downstream crates publicly re-export or expose upstream crate types in their public APIs:
     * **Breaking changes (`minor` bump) to `font-types`** affect: `read-fonts`, `write-fonts`, `skrifa`, `skera`, and `incremental-font-transfer`.
     * **Breaking changes (`minor` bump) to `read-fonts`** affect: `write-fonts`, `skrifa`, `skera`, and `incremental-font-transfer`.
     * **Breaking changes (`minor` bump) to `write-fonts`** affect: `skera` (which exposes `write_fonts::FontBuilder` in its public `Subset` trait). Note that `incremental-font-transfer` also depends on `write-fonts` internally, so if `write-fonts` has a breaking bump, `incremental-font-transfer` will also need a release if you want all workspace crates on the same `write-fonts` version.
     * **Breaking changes (`minor` bump) to `skera`** affect: `incremental-font-transfer` (which exposes `skera::serialize::SerializeErrorFlags` in its public `PatchingError` enum).
     * **Breaking changes (`minor` bump) to `skrifa`** affect: `skera` and `incremental-font-transfer` (which depend on `skrifa`).

## Release Steps

1. Determine which crates need to be published and their version bumps (see [Determining Versions and Crates to Release](#determining-versions-and-crates-to-release)).
1. Update manifest versions and release. `./resources/scripts/bump-version.sh` orchestrates this process.
   * `cargo release` does all the heavy lifting

   ```shell
   # To see usage
   ./resources/scripts/bump-version.sh
   # To do the thing
   ./resources/scripts/bump-version.sh read-fonts write-fonts patch
   ```

1. Commit these changes to a new branch, get it approved and merged, and switch
   to the up-to-date `main`.
1. Publish the crates. `./resources/scripts/release.sh` orchestrates the process.
   * You will be prompted to review changes along the way

   ```shell
   # To see usage
   ./resources/scripts/release.sh
   # To do the thing
   ./resources/scripts/release.sh read-fonts write-fonts
   ```

[`cargo-release`]: https://github.com/crate-ci/cargo-release
