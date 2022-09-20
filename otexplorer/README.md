# otexplorer

`otexplorer` is a Rust binary for printing and querying the contents of font
files.

It is loosely modeled off of the [ttx tool][ttx], although it uses a custom text
format instead of xml. In addition, it supports a query syntax, allowing
printing of a specific subtable or record.

## query syntax

the `-q` flag can be passed at the command line to specify a query string, which
represents a path within a font table.

Queries start with an OpenType tag indicating the root table, and then zero or
more dot-separated path elements; these can be either the names of fields, or if
a field is an array, an index into a field.

For example, to print the first subtable of the second GPOS lookup, you could
use the query,

```sh
-q GPOS.lookup_list.lookup_offsets.1.subtable_offsets.0
```

Queries are case-insensitive, and are fuzzily matched. The following queries are
equivalent:

```sh
-q GPOS.lookupListOffset.lookupOffsets.1.subtableOffsets.0
-q GPOS.look.off.1.off.0
```
