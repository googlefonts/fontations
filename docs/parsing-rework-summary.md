# Reworking the parsing framework — summary

The long version, with the reasoning and the code, is
[parsing-rework.md](parsing-rework.md). What it would cost Skia is
[parsing-rework-skia-impact.md](parsing-rework-skia-impact.md).

## The problem

Codegen describes a table well and a record badly. **A record is not a small
table**: a table is read from data beginning at its own first byte, while a
record lives at a position inside data it does not own, with offsets measured
from *that* data's start. One trait was describing the first and being made to
describe the second.

The bill comes as tables that had to be hand-written, or bent into a shape the
emitter could swallow:

| | what went wrong |
| --- | --- |
| GPOS `ValueRecord` | needs its parent to resolve device offsets; hand-written twice over, in two incompatible forms |
| fvar `InstanceRecord` | size is whatever `instanceSize` says; a trailing field exists only if it fits. Hand-written in full |
| fvar `AxisInstanceArrays` | a shim table with no counterpart in the spec, invented to be an offset's target |
| CFF `INDEX` | an empty one is two bytes, but `MIN_SIZE` came out as three, so it would not parse at all |

Separately, reading eagerly costs. A subsetter scanning `PairPosFormat2` built
two whole value records per class pair just to OR together a format.

## The design

**One trait for tables. None for records.**

```rust
pub trait Table<'a>: Sized {
    type Args: Copy;
    const MIN_SIZE: usize;
    fn read_with_args(data: Bytes<'a>, args: Self::Args) -> Option<Self>;
}
```

A record needs no trait, because its two questions already have homes: *how big
is it* is the size taxonomy the crate already has, and *how do I build one* is
`ArrayElement::read`. How the size is known decides everything else:

| byte length | trait | what it is handed |
| --- | --- | --- |
| compile time | `FixedSize` | `&'a Self`, borrowed straight out of a slice |
| computed from read args | `ComputedSize` | its parent, plus its own position |
| read from the data | `VariableSize` | a slice at itself — so it is read as a `Table` |

The middle row is the one that changes things. A record handed `(parent,
position)` can hold offsets measured against the enclosing table, which is
exactly what `ValueRecord` needed, and it reads nothing when it is built — so
walking records is free until you ask for a field.

**Accessors return `Option`, and it never nests.** The crate's existing
three-way rule survives, with the `Result` dropped:

| field | returns |
| --- | --- |
| covered by `MIN_SIZE` | bare `T` — `read` already checked |
| beyond `MIN_SIZE`, not conditional | bare `T`, empty or zero if short |
| conditional | `Option<T>` |

An offset resolves to `Option<T>` whether or not it is declared nullable,
because that declaration says what a *well formed* font contains, not what the
one in front of you does. `Option<Result<T, ReadError>>` disappears everywhere,
and `Option<Table>` is 16 bytes against `Result`'s 24.

**`WithParent<'a, R>`** pairs a borrowed fixed-size record with the base its
offsets are measured from, so `record.mark_anchor()` takes no `data` argument
that a caller could get wrong. The raw `&'a [R]` is still there underneath.

**One `Array`**, over four stores: elements at a computed stride, borrowed out
of a slice, behind a table of offsets, or walked because they describe their own
length.

## Buying back the errors

Dropping `Result` did not lose the information — it moved it somewhere better. A
field's extent *is* its generated `*_byte_range`, so a pass can walk the whole
graph and report everything wrong at once, with the path that leads to it:

```text
MarkBasePosFormat1.mark_array_offset → MarkArray.mark_records[0].mark_anchor_offset:
  offset 65520 does not resolve to a readable table
```

Two passes, not one with a flag, because the difference is what gets *linked*:

- `sanitize` — every problem, with names and paths. 9.5kB of string literals.
- `is_sound` — yes or no, stopping at the first problem. **No strings at all**,
  and no allocation.

Both behind features, both off by default. Cycles and depth are bounded by a
fixed-size decycler borrowed from skrifa; fan-out by a per-array budget; total
work by a table budget. A walk stopped by a limit reports the font as unsound,
never as sound.

## Where it stands

Codegen emits all of this today, behind a third mode, into a parallel tree —
so nothing existing is touched and the 80,000 lines of generated output stay
byte-identical.

```
cargo run -p font-codegen -- resources/codegen_plan.toml
cargo test -p read-fonts --features sanitize
```

**Generated so far:** GPOS, layout, variations, fvar and CFF — 100-odd types
from codegen inputs that were not modified for it, except where the input could
not say what the spec says. That covers every shape in the design, including all
four problem tables above. Each of them needed one small attribute:

| | |
| --- | --- |
| `#[record_size($arg)]` | the record occupies what a read arg says |
| `#[if_fits]` | a trailing field exists when the declared size leaves room |
| `#[at_offset($field)]` | this field starts where another field's offset points |
| `#[if_nonzero($field)]` | this field, and the rest, exist only when a count is non-zero |

Tests parse the spec's own example bytes and check the results against the
parser the crate ships, field for field.

## What is not done

- **No consumer uses it.** Pointing skrifa's GPOS path at it is the next real
  test — roughly 1,230 accessor call sites across skrifa, skera and ift. That is
  churn inside this workspace; Skia's direct use of read-fonts comes to four
  lines.
- **Traversal is not emitted**, deliberately.
- **write-fonts is untouched**, and is no longer expected to be a problem: its
  `from_obj` already discards the errors it would lose, so the migration looks
  like a small net simplification.
- **57 codegen inputs to go.** The emitter has not met `cmap`, `glyf`, `colr` or
  the AAT tables, and will grow — probably a few more narrow attributes with
  them.
- **Migration is not incremental.** Accessors are inherent methods, so the old
  and new shapes cannot share a name; it is flip-then-chase, or emit both with
  the new one suffixed.

## Open decisions

- Should a null offset in a non-nullable field fail sanitize? Both passes
  currently say yes, but fonts in the wild do it.
- Should the DSL grow a bare, untargeted offset? fvar currently has to name a
  target and then suppress the resolver.
- Does `Table` need `Args` for the handful of tables that use them as pure
  context rather than to size an array?
