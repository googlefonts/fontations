# codegen inputs

The files in this directory are not 'real' rust files; they are inputs to the
code generation tool (`font-codegen`, in the project root.)

Although they are not rust *syntax*, they contain only valid rust *tokens*,
which means that we can use crates like [`syn`] and [`quote`] to do our code
generation; (it also means that rust syntax highlighting works.)

For more information on codegen, see `font-codegen/README.md`.


[`syn`]: http://docs.rs/syn/
[`quote`]: http://docs.rs/quote/
