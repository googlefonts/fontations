# raw tables

This directory contains 'raw' text representations of font tables, which are
basically copy & pasted from the [Microsoft OpenType® docs][opentype].

These files are run through a sort of 'preprocessor', which turns them into
something that can be tokenized by the rust tokenizer; these outputs live in
`resources/codegen_inputs`.

For more information on codegen, see `font-codegen/README.md`.

[opentype]: https://docs.microsoft.com/en-us/typography/opentype/
