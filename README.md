# oxidize experiments

This repo contains some initial code exploring low-level parsing and access to
font data types in rust. See
[googlefonts/oxidize](https://github.com/googlefonts/oxidize) for more
background.

## contents

- `font-types` is an early attempt at a high-level API representing the core
  scalar types, and how to efficiently handle reading them.
- `font-codegen` tools used to generate code for parsing & manipulating various
  types in the spec. See `font-codegen/README.md` for more info.
- `read-fonts` contains tables and record definitions for parsing fonts.
- `write-fonts` contains tables and record definitions for editing and compiling
  fonts.

The `retired_crates` directory contains some earlier experiments:

- `toy-types` is a highly simplified version of this API. This is supposed to be
  easy to work with while experimenting with macros etc.
- `toy-types-derive` contains a derive macro that can be used to describe types
  which represent various font tables and records.
- `raw-types` is a set of simple zerocopy types, intended to be the basis for,
- `font-tables` is an earlier crate that was subsequently split into
  `read-fonts` and `write-fonts`.


## codegen

This crate relies heavily on automatically generated code. For an overview of
how this works, see `font-codegen/README.md`.
