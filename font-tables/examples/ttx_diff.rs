//! print the ttx diff of two font files

use std::path::Path;

/// max non-diff lines to print; after this we skip to the next difference.
const MAX_SAME_TO_PRINT: usize = 100;

fn main() {
    let args = match flags::Args::from_env() {
        Ok(args) => args,
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(1);
        }
    };

    let ttx1 = get_ttx(&args.path1, &args.tables);
    let ttx2 = get_ttx(&args.path2, &args.tables);

    assert_eq_str!(ttx1, ttx2)
}

fn get_ttx(path: &Path, args: &[String]) -> String {
    eprintln!("getting ttx for path {}, tables {args:?}", path.display());
    let ttx_path = path.with_extension("ttx");
    let _ = std::fs::remove_file(&ttx_path);
    let mut cmd = std::process::Command::new("ttx");
    for table in args {
        cmd.arg("-t").arg(table);
    }
    let status = cmd.arg(path).output().unwrap();
    if !status.status.success() {
        panic!("ttx dump failed");
    }

    assert!(ttx_path.exists());

    let r = std::fs::read_to_string(&ttx_path).unwrap();
    std::fs::remove_file(&ttx_path).unwrap();
    eprintln!("done :)");
    r
}

mod flags {
    use std::path::PathBuf;

    xflags::xflags! {
        /// Generate font table representations
        cmd args
            required path1: PathBuf
            required path2: PathBuf
            {
                repeated -t, --tables tables: String
            }

    }
}

mod pretty_diff {
    use std::fmt;

    use ansi_term::{
        Colour::{Fixed, Green, Red},
        Style,
    };

    use super::MAX_SAME_TO_PRINT;

    const SIGN_RIGHT: char = '>'; // + > →
    const SIGN_LEFT: char = '<'; // - < ←

    #[macro_export]
    macro_rules! assert_eq_str {
    ($left:expr, $right:expr$(,)?) => ({
        assert_eq_str!(@ $left, $right, "", "");
    });
    ($left:expr, $right:expr, $($arg:tt)*) => ({
        assert_eq_str!(@ $left, $right, ": ", $($arg)+);
    });
    (@ $left:expr, $right:expr, $maybe_semicolon:expr, $($arg:tt)*) => ({
        match (&($left), &($right)) {
            (left, right) => {
                if !(*left == *right) {
                    ::std::panic!("assertion failed: `(left == right)`{}{}\
                       \n\
                       \n{}\
                       \n",
                       $maybe_semicolon,
                       format_args!($($arg)*),
                       pretty_diff::MyStrs { left, right }
                    )
                }
            }
        }
    });
}

    macro_rules! paint {
    ($f:expr, $colour:expr, $fmt:expr, $($args:tt)*) => (
        write!($f, "{}", $colour.paint(format!($fmt, $($args)*)))
    )
}

    pub(crate) struct MyStrs<'a> {
        pub(crate) left: &'a str,
        pub(crate) right: &'a str,
    }

    impl std::fmt::Display for MyStrs<'_> {
        fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
            write_line_diff(f, self.left, self.right)
        }
    }

    /// Present the diff output for two mutliline strings in a pretty, colorised manner.
    pub fn write_line_diff<TWrite: fmt::Write>(
        f: &mut TWrite,
        left: &str,
        right: &str,
    ) -> fmt::Result {
        let diff = ::diff::lines(left, right);

        let mut changes = diff.into_iter().peekable();
        let mut previous_deletion = LatentDeletion::default();
        let mut seen_same = 0usize;

        let flush_seen = |f: &mut TWrite, seen: &mut usize| {
            if *seen > MAX_SAME_TO_PRINT {
                writeln!(f, "skipped {} lines", *seen - MAX_SAME_TO_PRINT).unwrap();
            }
            *seen = 0;
        };

        while let Some(change) = changes.next() {
            match (change, changes.peek()) {
                // If the text is unchanged, just print it plain
                (::diff::Result::Both(value, _), _) => {
                    previous_deletion.flush(f)?;
                    seen_same += 1;
                    if seen_same < MAX_SAME_TO_PRINT {
                        writeln!(f, " {}", value)?;
                    }
                }
                // Defer any deletions to next loop
                (::diff::Result::Left(deleted), _) => {
                    flush_seen(f, &mut seen_same);
                    previous_deletion.flush(f)?;
                    previous_deletion.set(deleted);
                }
                // Underlying diff library should never return this sequence
                (::diff::Result::Right(_), Some(::diff::Result::Left(_))) => {
                    panic!("insertion followed by deletion");
                }
                // If we're being followed by more insertions, don't inline diff
                (::diff::Result::Right(inserted), Some(::diff::Result::Right(_))) => {
                    flush_seen(f, &mut seen_same);
                    previous_deletion.flush(f)?;
                    paint!(f, Green, "{}{}", SIGN_RIGHT, inserted)?;
                    writeln!(f)?;
                }
                // Otherwise, check if we need to inline diff with the previous line (if it was a deletion)
                (::diff::Result::Right(inserted), _) => {
                    flush_seen(f, &mut seen_same);
                    if let Some(deleted) = previous_deletion.take() {
                        write_inline_diff(f, deleted, inserted)?;
                    } else {
                        previous_deletion.flush(f)?;
                        paint!(f, Green, "{}{}", SIGN_RIGHT, inserted)?;
                        writeln!(f)?;
                    }
                }
            };
        }

        flush_seen(f, &mut seen_same);
        previous_deletion.flush(f)?;
        Ok(())
    }

    /// Delay formatting this deleted chunk until later.
    ///
    /// It can be formatted as a whole chunk by calling `flush`, or the inner value
    /// obtained with `take` for further processing.
    #[derive(Default)]
    struct LatentDeletion<'a> {
        // The most recent deleted line we've seen
        value: Option<&'a str>,
        // The number of deleted lines we've seen, including the current value
        count: usize,
    }

    impl<'a> LatentDeletion<'a> {
        /// Set the chunk value.
        fn set(&mut self, value: &'a str) {
            self.value = Some(value);
            self.count += 1;
        }

        /// Take the underlying chunk value, if it's suitable for inline diffing.
        ///
        /// If there is no value of we've seen more than one line, return `None`.
        fn take(&mut self) -> Option<&'a str> {
            if self.count == 1 {
                self.value.take()
            } else {
                None
            }
        }

        /// If a value is set, print it as a whole chunk, using the given formatter.
        ///
        /// If a value is not set, reset the count to zero (as we've called `flush` twice,
        /// without seeing another deletion. Therefore the line in the middle was something else).
        fn flush<TWrite: fmt::Write>(&mut self, f: &mut TWrite) -> fmt::Result {
            if let Some(value) = self.value {
                paint!(f, Red, "{}{}", SIGN_LEFT, value)?;
                writeln!(f)?;
                self.value = None;
            } else {
                self.count = 0;
            }

            Ok(())
        }
    }

    /// Group character styling for an inline diff, to prevent wrapping each single
    /// character in terminal styling codes.
    ///
    /// Styles are applied automatically each time a new style is given in `write_with_style`.
    struct InlineWriter<'a, Writer> {
        f: &'a mut Writer,
        style: Style,
    }

    impl<'a, Writer> InlineWriter<'a, Writer>
    where
        Writer: fmt::Write,
    {
        fn new(f: &'a mut Writer) -> Self {
            InlineWriter {
                f,
                style: Style::new(),
            }
        }

        /// Push a new character into the buffer, specifying the style it should be written in.
        fn write_with_style(&mut self, c: &char, style: Style) -> fmt::Result {
            // If the style is the same as previously, just write character
            if style == self.style {
                write!(self.f, "{}", c)?;
            } else {
                // Close out previous style
                write!(self.f, "{}", self.style.suffix())?;

                // Store new style and start writing it
                write!(self.f, "{}{}", style.prefix(), c)?;
                self.style = style;
            }
            Ok(())
        }

        /// Finish any existing style and reset to default state.
        fn finish(&mut self) -> fmt::Result {
            // Close out previous style
            writeln!(self.f, "{}", self.style.suffix())?;
            self.style = Default::default();
            Ok(())
        }
    }

    /// Format a single line to show an inline diff of the two strings given.
    ///
    /// The given strings should not have a trailing newline.
    ///
    /// The output of this function will be two lines, each with a trailing newline.
    fn write_inline_diff<TWrite: fmt::Write>(
        f: &mut TWrite,
        left: &str,
        right: &str,
    ) -> fmt::Result {
        let diff = ::diff::chars(left, right);
        let mut writer = InlineWriter::new(f);

        // Print the left string on one line, with differences highlighted
        let light = Red.into();
        let heavy = Red.on(Fixed(52)).bold();
        writer.write_with_style(&SIGN_LEFT, light)?;
        for change in diff.iter() {
            match change {
                ::diff::Result::Both(value, _) => writer.write_with_style(value, light)?,
                ::diff::Result::Left(value) => writer.write_with_style(value, heavy)?,
                _ => (),
            }
        }
        writer.finish()?;

        // Print the right string on one line, with differences highlighted
        let light = Green.into();
        let heavy = Green.on(Fixed(22)).bold();
        writer.write_with_style(&SIGN_RIGHT, light)?;
        for change in diff.iter() {
            match change {
                ::diff::Result::Both(value, _) => writer.write_with_style(value, light)?,
                ::diff::Result::Right(value) => writer.write_with_style(value, heavy)?,
                _ => (),
            }
        }
        writer.finish()
    }
}
