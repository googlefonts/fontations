//! Buying back the errors the read path gave up.
//!
//! Accessors return `Option` and default rather than reporting what went wrong,
//! which is what makes them cheap. Nothing is lost, though — it is only
//! deferred. Everything an accessor could have complained about is still
//! visible from outside: a field's extent is exactly what its generated
//! `*_byte_range` says, and whether an offset resolves is a question anyone can
//! ask.
//!
//! So the diagnosis moves into a separate pass. [`sanitize`] walks the whole
//! table graph from any starting table, checks every field of every table it
//! reaches, and reports each problem with the path that leads to it and the
//! name of the field:
//!
//! ```text
//! Gpos.lookup_list → LookupList.lookups[3] → PairPosFormat2.class1_records
//!   array extends past the end of the table (needs 4820, have 312)
//! ```
//!
//! This is not the read path's error handling relocated. It is a different
//! thing that is better at the job: a caller who wants to know whether a font
//! is well formed asks once, up front, instead of threading a `Result` through
//! every accessor and still only learning about the first field they happened
//! to touch.
//!
//! This is the pass that says *why*, and it pays for that in strings: every
//! table and field it can name is a literal in the binary. A caller that only
//! wants a yes or no should use [`is_sound`][super::fast_sanitize::is_sound],
//! which is a separate walk precisely so that none of those literals are
//! linked.

#![deny(clippy::arithmetic_side_effects)]

use alloc::vec::Vec;
use core::ops::Range;

use super::bytes::Bytes;
use super::decycler::{Decycler, Rejected};

/// How far into the graph a walk will go before giving up.
///
/// Offsets can point anywhere, including at each other, so a walk needs bounds.
/// Cycles and depth are handled by the [`Decycler`], which needs no
/// configuration; these bound the rest.
#[derive(Clone, Copy, Debug)]
pub struct Limits {
    /// The most tables to visit.
    ///
    /// The whole-graph bound. Offsets are a graph, not a tree, so a font can
    /// point at the same subtable from many places; already-visited tables are
    /// skipped, and this caps what is left.
    pub tables: usize,
    /// The most errors to collect before stopping.
    pub errors: usize,
    /// The most elements of any one array to descend into.
    ///
    /// The fan-out bound. The array's own extent is always checked, so a count
    /// larger than the data is still reported; this caps only how many elements
    /// are *walked*, so a font claiming a million subtables costs one check
    /// rather than a million.
    pub array_elements: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            tables: 1 << 16,
            errors: 64,
            array_elements: 1 << 14,
        }
    }
}

/// One step in the path to an error.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Step<'a> {
    /// Entered a table of this type.
    Table(&'a str),
    /// Entered this field.
    Field(&'a str),
    /// Entered this element of the field above.
    Index(usize),
}

/// What went wrong.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Problem {
    /// A field's bytes are not all present.
    ///
    /// For an array this is the case the read path swallows: the accessor
    /// returns no elements, and without this pass nothing says the count was
    /// larger than the data.
    FieldOutOfBounds { needed: usize, available: usize },
    /// An offset that is not declared nullable is null.
    ///
    /// Legal in practice — fonts do it — so the read side treats it as absent.
    /// It is still worth reporting to anyone asking whether a font is well
    /// formed.
    NullOffset,
    /// An offset does not lead to a readable table.
    ///
    /// Out of bounds, too short for the table's own header, or — for a table
    /// chosen by a format word — a format no variant matches. The read side
    /// gives `None` for all three, and so does this: the offset is where the
    /// trail goes cold, and saying which of the three would mean the resolving
    /// accessor reporting rather than returning.
    UnresolvableOffset { offset: u32 },
    /// An offset leads back to a table already on the path to it.
    Cycle,
    /// The walk stopped here rather than going deeper.
    LimitReached(&'static str),
}

impl core::fmt::Display for Problem {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Problem::FieldOutOfBounds { needed, available } => write!(
                f,
                "field extends past the end of the table (needs {needed}, have {available})"
            ),
            Problem::NullOffset => write!(f, "offset is null but is not declared nullable"),
            Problem::UnresolvableOffset { offset } => {
                write!(f, "offset {offset} does not resolve to a readable table")
            }
            Problem::Cycle => write!(f, "offset leads back to a table already on the path"),
            Problem::LimitReached(what) => write!(f, "walk stopped: {what} limit reached"),
        }
    }
}

/// A problem, and the path that leads to it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Error {
    path: Vec<Step<'static>>,
    problem: Problem,
}

impl Error {
    /// The path from the starting table to where the problem is.
    pub fn path(&self) -> &[Step<'static>] {
        &self.path
    }

    pub fn problem(&self) -> Problem {
        self.problem
    }

    /// The name of the field the problem is in, if it is in one.
    pub fn field(&self) -> Option<&'static str> {
        self.path.iter().rev().find_map(|step| match step {
            Step::Field(name) => Some(*name),
            _ => None,
        })
    }

    /// The type of the table the problem is in.
    pub fn table(&self) -> Option<&'static str> {
        self.path.iter().rev().find_map(|step| match step {
            Step::Table(name) => Some(*name),
            _ => None,
        })
    }
}

impl core::fmt::Display for Error {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let mut first = true;
        for step in &self.path {
            match step {
                Step::Table(name) => {
                    if !first {
                        write!(f, " → ")?;
                    }
                    write!(f, "{name}")?;
                }
                Step::Field(name) => write!(f, ".{name}")?,
                Step::Index(i) => write!(f, "[{i}]")?,
            }
            first = false;
        }
        write!(f, ": {}", self.problem)
    }
}

/// What a walk found.
#[derive(Clone, Debug, Default)]
pub struct Report {
    errors: Vec<Error>,
    /// The first problem found, in either mode.
    first: Option<Problem>,
    failed: bool,
    tables_visited: usize,
    stopped_early: bool,
}

impl Report {
    /// `true` if nothing was wrong.
    ///
    pub fn is_ok(&self) -> bool {
        !self.failed
    }

    /// Every problem found, each with its path.
    pub fn errors(&self) -> &[Error] {
        &self.errors
    }

    /// The first problem found, without its path.
    pub fn first_problem(&self) -> Option<Problem> {
        self.first
    }

    /// How many tables the walk reached.
    pub fn tables_visited(&self) -> usize {
        self.tables_visited
    }

    /// `true` if a limit stopped the walk before it had seen everything, so an
    /// empty error list does not mean the font is clean.
    pub fn stopped_early(&self) -> bool {
        self.stopped_early
    }
}

impl core::fmt::Display for Report {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        if !self.failed {
            return write!(f, "{} tables, no problems", self.tables_visited);
        }
        writeln!(
            f,
            "{} problem(s) in {} tables:",
            self.errors.len(),
            self.tables_visited
        )?;
        for error in &self.errors {
            writeln!(f, "  {error}")?;
        }
        Ok(())
    }
}

/// The state a walk carries.
///
/// Generated code drives this; there is no need to call it by hand.
pub struct Context {
    limits: Limits,
    path: Vec<Step<'static>>,
    report: Report,
    /// Stops the walk looping, and bounds its depth.
    ///
    /// A node is where a table's data starts plus what type it was read as: the
    /// same bytes read as two different tables are two different nodes.
    decycler: Decycler<(usize, usize)>,
    /// Set when a limit says the walk should stop.
    done: bool,
}

impl Context {
    pub fn new(limits: Limits) -> Self {
        Self {
            limits,
            path: Vec::new(),
            report: Report::default(),
            decycler: Decycler::new(),
            done: false,
        }
    }

    /// `true` when the walk should unwind without doing anything else.
    ///
    /// Generated code relies on `enter_table` and `element_budget` refusing to
    /// go further, so it rarely needs this directly.
    #[inline]
    pub fn is_done(&self) -> bool {
        self.done
    }

    pub fn finish(self) -> Report {
        self.report
    }

    /// Records a problem at the current path.
    pub fn report(&mut self, problem: Problem) {
        if self.done {
            return;
        }
        self.report.failed = true;
        self.report.first.get_or_insert(problem);
        if self.report.errors.len() >= self.limits.errors {
            self.report.stopped_early = true;
            self.done = true;
            return;
        }
        self.report.errors.push(Error {
            path: self.path.clone(),
            problem,
        });
    }

    /// Begins a table.
    ///
    /// Returns `false` if this table has already been walked, or a limit has
    /// been reached, in which case the caller must not descend.
    pub fn enter_table(&mut self, name: &'static str, data: Bytes) -> bool {
        if self.done {
            return false;
        }
        if self.report.tables_visited >= self.limits.tables {
            self.report.stopped_early = true;
            self.report(Problem::LimitReached("table"));
            return false;
        }
        let id = (data.as_bytes().as_ptr() as usize, name.as_ptr() as usize);
        match self.decycler.enter(id) {
            Ok(()) => {}
            Err(Rejected::Cycle) => {
                self.report(Problem::Cycle);
                return false;
            }
            Err(Rejected::TooDeep) => {
                self.report.stopped_early = true;
                self.report(Problem::LimitReached("depth"));
                return false;
            }
        }
        self.report.tables_visited = self.report.tables_visited.saturating_add(1);
        self.path.push(Step::Table(name));
        true
    }

    pub fn exit_table(&mut self) {
        self.decycler.exit();
        debug_assert!(matches!(self.path.last(), Some(Step::Table(_))));
        self.path.pop();
    }

    #[inline]
    pub fn enter_field(&mut self, name: &'static str) {
        self.path.push(Step::Field(name));
    }

    #[inline]
    pub fn exit_field(&mut self) {
        debug_assert!(matches!(self.path.last(), Some(Step::Field(_))));
        self.path.pop();
    }

    #[inline]
    pub fn enter_index(&mut self, index: usize) {
        self.path.push(Step::Index(index));
    }

    #[inline]
    pub fn exit_index(&mut self) {
        debug_assert!(matches!(self.path.last(), Some(Step::Index(_))));
        self.path.pop();
    }

    /// How many elements of an array to walk.
    ///
    /// Zero once a limit has stopped the walk, so a loop over it does not run.
    #[inline]
    pub fn element_budget(&self, len: usize) -> usize {
        if self.done {
            return 0;
        }
        len.min(self.limits.array_elements)
    }

    /// Checks that a field's bytes are all present.
    ///
    /// This is the check the read path stopped making: an array whose extent
    /// runs past the data reads as empty, and nothing else would say so.
    #[inline]
    pub fn check_extent(&mut self, name: &'static str, range: Range<usize>, data: Bytes) {
        if self.done {
            return;
        }
        if range.end > data.len() {
            self.enter_field(name);
            self.report(Problem::FieldOutOfBounds {
                needed: range.end,
                available: data.len(),
            });
            self.exit_field();
        }
    }

    /// Reports an offset that did not resolve.
    ///
    /// `nullable` says what the font is allowed to contain, which decides
    /// whether a zero is worth mentioning.
    #[inline]
    pub fn check_offset(
        &mut self,
        name: &'static str,
        offset: u32,
        resolved: bool,
        nullable: bool,
    ) {
        if resolved || self.done {
            return;
        }
        self.enter_field(name);
        if offset == 0 {
            if !nullable {
                self.report(Problem::NullOffset);
            }
        } else {
            self.report(Problem::UnresolvableOffset { offset });
        }
        self.exit_field();
    }
}

/// A table that can check itself and everything it points at.
///
/// Generated alongside the table. Implementing it by hand is only needed for
/// the handful of types that are hand-written.
pub trait Sanitize<'a> {
    /// The name used in error paths.
    const TYPE_NAME: &'static str;

    /// Checks this table's own fields and descends into what it points at.
    ///
    /// Callers should use [`sanitize`] or [`sanitize_with`] instead; this is
    /// the recursive half, and expects the context's path to already be
    /// positioned.
    fn sanitize_in(&self, ctx: &mut Context);
}

/// Walks the whole graph from `table` and reports everything wrong with it,
/// each problem with the path that leads to it.
///
/// For showing someone what is wrong. If you only need a yes or no, use
/// [`is_sound`][super::fast_sanitize::is_sound], which is a separate walk that
/// links none of the names this one carries.
pub fn sanitize<'a, T: Sanitize<'a>>(table: &T) -> Report {
    sanitize_with(table, Limits::default())
}

/// As [`sanitize`], with explicit limits.
pub fn sanitize_with<'a, T: Sanitize<'a>>(table: &T, limits: Limits) -> Report {
    let mut ctx = Context::new(limits);
    table.sanitize_in(&mut ctx);
    ctx.finish()
}
