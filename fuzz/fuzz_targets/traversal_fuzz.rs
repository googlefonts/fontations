use std::fmt::Debug;
use std::io::Write;

use libfuzzer_sys::Corpus;
use read_fonts::FontRead;

/// Reusable entry point to fuzz any table via traversal
///
/// To debug timeouts, set `print_output` to `true` (the output text will help
/// show where you're hitting a loop)
pub fn try_traverse_table<'a, T: FontRead<'a> + Debug>(
    data: &'a [u8],
    print_output: bool,
) -> Corpus {
    match T::read(data.into()) {
        Err(_) => Corpus::Reject,
        Ok(table) => {
            if print_output {
                let _ = writeln!(std::io::stderr(), "{table:?}");
            } else {
                // if we don't want to see the output don't bother filling a buffer
                let mut empty = std::io::empty();
                write!(&mut empty, "{table:?}").unwrap();
            }
            Corpus::Keep
        }
    }
}
