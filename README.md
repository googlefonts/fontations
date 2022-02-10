# oxidize experiments

This repo contains some initial code exploring low-level parsing and access to
font data types in rust. See
[googlefonts/oxidize](https://github.com/googlefonts/oxidize) for more
background.

## contents

- `font-types` is an early attempt at a high-level API representing the core
  scalar types, and how to efficiently handle reading them.
- `toy-types` is a highly simplified version of this API. This is supposed to be
  easy to work with while experimenting with macros etc.
- `toy-types-derive` contains a derive macro that can be used to describe types
  which represent various font tables and records.
- `raw-types` is a set of simple zerocopy types, intended to be the basis for,
- `toy-table-macro` contains a function-like proc macro that implements a DSL
  for describing arbitrary font-like-things.
