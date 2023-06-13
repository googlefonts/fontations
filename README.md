# Fontations

This repo contains a number of foundational crates for reading and
manipulating OpenType font files. It is motivated by a desire to have more
robust and performant open tools for a variety of font engineering and
production tasks. For an overview of the motivations, see
[googlefonts/oxidize][oxidize].


## structure

Currently, this repo contains three main library crates: [`font-types`][], [`read-fonts`][],
and [`write-fonts`][], in addition to one binary crate, [`otexplorer`][]:

- `font-types` contains common definitions of the core types used in the
  OpenType spec. This is a small crate, and is intended as a basic dependency
  for any project reading or manipulating font data.
- [`read-fonts`][] contains code for parsing and accessing font files. It is
  intended to be a high performance parser, suitable for shaping. In particular
  this means that it performs no allocation and no copying.
- [`write-fonts`][] contains code for modifying and writing font data. It contains
  owned types representing the various tables and records in the specification,
  as well as code for compiling these and writing out font files. It has an
  optional dependency on `read-fonts`, in which case it can also parse font
  data, which can then be modified and written back out to disk.
- [`otexplorer`][] is a binary crate for exploring the contents of font files.
  It is developed as a debugging tool, and may also be useful as an example of
  how the [`read-fonts`][] crate can be used.

## codegen

Much of the code in the `read-fonts` and `write-fonts` crate is generated
automatically. Code generation is performed by the `font-codegen` crate. For an
overview of what we generate and how it works, see the [codegen-tour][]. For an
overview of how to use the `font-codegen` crate, see the readme at
[`font-codegen/README.md`][codegen-readme].

## contributing

We have included a few git hooks that you may choose to use to ensure that
patches will pass CI; these are in `resources/githooks`.

If you would like to have these run automatically when you commit or push
changes, you can set this as your git hooksPath:

```sh
git config core.hooksPath "./resources/githooks"
```

**note**: If you wish to use the hooks on macOS, install the gnu coreutils
(`brew install coreutils`, via homebrew.)

## releasing

We use [`cargo-release`] to help guide the release process. It can be installed
with `cargo install cargo-release`. You may need to install `pkg-config` via your
package manager for this to work.

Releasing involves the following steps:

1. Determine which crates may need to be published: run `cargo release changes`
   to see which crates have been modified since their last release.
1. Determine the new versions for the crates.
   * Before 1.0, breaking changes bump the *minor* version number, and non-breaking changes modify the *patch* number.
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

## skia and chromium builds

An experimental `SkTypeface` implementation based on Fontations exists. This
build is available in the Skia and Chromium repositories. The goal is to
eventually use Fontations + Skia in Chromium as a memory-safe font
backend. Tracking bugs: [Skia](https://crbug.com/skia/14259),
[Chromium](https://crbug.com/1446251). To build the backends in Skia or Chromium
follow the instructions below. This process is currently only tested on Linux.

### skia build

1. Download Skia https://skia.org/docs/user/download/
1. `$ bazel build --sandbox_base=/dev/shm --with_fontations //tools/viewer
   //tests:FontationsTest` (to build debug, add `-c dbg` after `build`)

#### unit tests and GM

Run unit tests withstayeri

`$ bazel-bin/tests/FontationsTest`

Run Fontations Skia GM using:

`$ bazel-bin/tools/viewer/viewer --slide GM_typeface_fontations_roboto`

### chromium build

1. Follow the instructions for [getting and building
   Chromium](https://chromium.googlesource.com/chromium/src/+/main/docs/linux/build_instructions.md)
1. Verify that https://chromium-review.googlesource.com/c/chromium/src/+/4608308
   has landed or run `git cl apply 4608308`.
1. Add `use_typeface_fontations = true` to your `args.gn` using `$ gn args
   out/<builddir>`
1. Comment out `"-Dunsafe_op_in_unsafe_fn",` in `build/config/compiler/BUILD.gn`
   if https://bugs.chromium.org/p/chromium/issues/detail?id=1448457 is not
   closed yet.
1. Build Chromium using `autoninja -C out/<builddir> chrome`
1. Run Chromium using `$ out/<builddir>/chrome`

[codegen-readme]: ./font-codegen/README.md
[`read-fonts`]: ./read-fonts
[`font-types`]: ./font-types
[`write-fonts`]: ./write-fonts
[`otexplorer`]: ./otexplorer
[oxidize]: https://github.com/googlefonts/oxidize
[codegen-tour]: ./docs/codegen-tour.md
[`cargo-release`]: https://github.com/crate-ci/cargo-release
