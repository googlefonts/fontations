[![CI Status](https://github.com/googlefonts/fontations/actions/workflows/rust.yml/badge.svg)](https://github.com/googlefonts/fontations/actions/workflows/rust.yml)
[![Fuzzing Status](https://oss-fuzz-build-logs.storage.googleapis.com/badges/fontations.svg)](https://bugs.chromium.org/p/oss-fuzz/issues/list?sort=-opened&can=1&q=proj:fontations)



# Fontations

This repo contains a number of foundational crates for reading and
manipulating OpenType font files. It is motivated by a desire to have more
robust and performant open tools for a variety of font engineering and
production tasks. For an overview of the motivations, see
[googlefonts/oxidize][oxidize].

## Structure

Currently, this repo contains four main library crates: [`font-types`][], [`read-fonts`][],
[`write-fonts`][], and [`skrifa`][]:

- [`font-types`][] contains common definitions of the core types used in the
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
- [`skrifa`][] is a mid level library that provides access to various types of
  metadata contained in a font as well as support for loading glyph outlines.
    - It's primary purpose is to replace FreeType in Google applications.
    - It is also used for tasks such as produce assets for https://fonts.google.com/icons

## depgraph

Shows the non-dev dependency relationships among the font-related crates in fontations. fontc primarily uses the fontations crates via write-fonts and skrifa.

```mermaid
%% This is a map of non-dev font-related dependencies.
%% See https://mermaid.live/edit for a lightweight editing environment for
%% mermaid diagrams.

graph LR
    %% First we define the nodes and give them short descriptions.
    %% We group them into subgraphs by repo so that the visual layout
    %% maps to the source layout, as this is intended for contributors.
    %% https://github.com/googlefonts/fontations
    subgraph fontations[fontations repo]
        fauntlet[fauntlet\ncompares Skrifa & freetype]
        font-types[font-types\ndefinitions of types\nfrom OpenType and a bit more]
        read-fonts[read-fonts\nparses and reads OpenType fonts]
        skrifa[skrifa\nhigher level lib\nfor reading OpenType fonts]
        write-fonts[write-fonts\ncreates and edits font-files]
    end

    %% https://github.com/linebender/kurbo
    kurbo[kurbo\n2d curves lib]
    %% https://github.com/linebender/norad
    norad[norad\nhandles Unified Font Object files]
    %% https://github.com/PistonDevelopers/freetype-rs
    freetype-rs[freetype-rs\nbindings for the FreeType library]

    %% Now define the edges.
    %% Made by hand on March 20, 2024, probably not completely correct.
    %% Should be easy to automate if we want to, main thing is to
    %% define the crates of interest.
    fauntlet --> skrifa
    fauntlet --> freetype-rs
    read-fonts --> font-types
    skrifa --> read-fonts
    write-fonts --> font-types
    write-fonts --> read-fonts
    write-fonts --> kurbo
    norad --> kurbo
```

## codegen

Much of the code in the `read-fonts` and `write-fonts` crate is generated
automatically. Code generation is performed by the `font-codegen` crate. For an
overview of what we generate and how it works, see the [codegen-tour][]. For an
overview of how to use the `font-codegen` crate, see the readme at
[`font-codegen/README.md`][codegen-readme].

## Fuzzing

* Coverage can be viewed at https://oss-fuzz.com/, and search for "fontations"
* The `fuzz/` crate in this repo contains our fuzzers
* fuzzers are implemented using the [`cargo-fuzz`](https://rust-fuzz.github.io/book/cargo-fuzz.html) crate
* [oss-fuzz](https://github.com/google/oss-fuzz) configuration lives in https://github.com/google/oss-fuzz/tree/master/projects/fontations
   * [build.sh](https://github.com/google/oss-fuzz/blob/master/projects/fontations/build.sh) looks for `target/x86_64-unknown-linux-gnu/release/fuzz_*`
   * ^ is meant to mean we can add additional fuzzers to fontations without having to touch oss-fuzz every time
   * `build.sh` also controls the test corpus, look for the `git clone` lines
 
To reproduce a fuzzer issue:

1. Download the file from the testcase, e.g. https://oss-fuzz.com/testcase-detail/6213391169945600
1. Build the fuzzers
   * `cargo +nightly  fuzz build -O --debug-assertions`
1. Pass the repro file to the fuzzer
   * `target/x86_64-unknown-linux-gnu/release/fuzz_skrifa_outline ~/Downloads/clusterfuzz-testcase-minimized-fuzz_skrifa_outline-6213391169945600`

## Performance

### Harfbuzz

https://github.com/harfbuzz/harfbuzz/blob/main/perf/README.md has instructions on
running harfbuzz benchmarks, including against Fontations.

## Contributing

We have included a few git hooks that you may choose to use to ensure that
patches will pass CI; these are in `resources/githooks`.

If you would like to have these run automatically when you commit or push
changes, you can set this as your git hooksPath:

```sh
git config core.hooksPath "./resources/githooks"
```

**note**: If you wish to use the hooks on macOS, install the gnu coreutils
(`brew install coreutils`, via homebrew.)

## Releasing

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

## Skia and Chromium builds

An experimental `SkTypeface` implementation based on Fontations exists. This
build is available in the Skia and Chromium repositories. The goal is to
eventually use Fontations + Skia in Chromium as a memory-safe font
backend. Tracking bugs: [Skia](https://crbug.com/skia/14259),
[Chromium](https://crbug.com/1446251). To build the backends in Skia or Chromium
follow the instructions below. This process is currently only tested on Linux.

### Skia build

1. Download Skia https://skia.org/docs/user/download/, including bazelisk
1. `$ bazelisk build --sandbox_base=/dev/shm --with_fontations //tools/viewer
   //tests:FontationsTest` (to build debug, add `-c dbg` after `build`)
   * You should now have executables at `bazel-bin/tests/FontationsTest` and `bazel-bin/tools/viewer/viewer`

#### Skia Fontations unit tests

Build as above, then run the executable to run tests:

`$ bazel-bin/tests/FontationsTest`

OR compile and test in one command:

`$ bazelisk  test --sandbox_base=/dev/shm --with_fontations //tests:FontationsTest`

#### Skia Fontations GM

Build as above then:

`$ bazel-bin/tools/viewer/viewer --slide GM_typeface_fontations_roboto`

OR build and run in one command:

`$ bazelisk run --sandbox_base=/dev/shm --with_fontations //tools/viewer -- --slide GM_typeface_fontations_roboto`

### chromium build

1. Follow the instructions for [getting and building
   Chromium](https://chromium.googlesource.com/chromium/src/+/main/docs/linux/build_instructions.md)
1. Add `use_typeface_fontations = true` to your `args.gn` using `$ gn args
   out/<builddir>`
1. Build Chromium using `autoninja -C out/<builddir> chrome`
1. Run Chromium using `$ out/<builddir>/chrome`
1. Go to chrome://flags/#enable-fontations-backend and activate the flag, restart Chrome.
1. Navigate to any URL, observe web fonts being displayed with Fontations + Skia path rendering.

[codegen-readme]: ./font-codegen/README.md
[`read-fonts`]: ./read-fonts
[`font-types`]: ./font-types
[`write-fonts`]: ./write-fonts
[`skrifa`]: ./skrifa
[oxidize]: https://github.com/googlefonts/oxidize
[codegen-tour]: ./docs/codegen-tour.md
[`cargo-release`]: https://github.com/crate-ci/cargo-release
