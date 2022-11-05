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
git config core.hooksPath "./git_hooks"
```


[codegen-readme]: ./font-codegen/README.md
[`read-fonts`]: ./read-fonts
[`font-types`]: ./font-types
[`write-fonts`]: ./write-fonts
[`otexplorer`]: ./otexplorer
[oxidize]: https://github.com/googlefonts/oxidize
[codegen-tour]: ./docs/codegen-tour.md
