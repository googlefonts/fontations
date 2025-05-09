# Fontations

Foundational crates for working with fonts.

This crate reexports compatible versions of the other crates in the fontations
project:

- [`font-types`][] (low-level data types, as `fontations::types`)
- [`read-fonts`][] (low-level code for parsing and accesing font tables, as
  `fontations::read`)
- [`write-fonts`][] (low-level code for modifying and generating font tables, as
  `fontations::write`)
- [`skrifa`][] (higher level abstractions for working with font data, as
  `fontations::skrifa`)

The goal is to make it easy to keep these dependencies in sync.

[`font-types`]: https://github.com/googlefonts/fontations/tree/main/font-types
[`read-fonts`]: https://github.com/googlefonts/fontations/tree/main/read-fonts
[`write-fonts`]: https://github.com/googlefonts/fontations/tree/main/write-fonts
[`skrifa`]: https://github.com/googlefonts/fontations/tree/main/skrifa


