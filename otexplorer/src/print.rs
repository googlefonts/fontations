//! pretty printing implementation

use std::io::Write;

use ansi_term::{Color, Style};
use read_fonts::traversal::{
    ArrayOffset, FieldType, OffsetType, ResolvedOffset, SomeArray, SomeString, SomeTable,
    StringOffset,
};

static MANY_SPACES: [u8; 200] = [0x20; 200];
// width of the left column, which contains the textual representation.
const L_COLUMN_WIDTH: usize = 62;
// position of array indexes, if they are printed
const ARRAY_POS_WIDTH: usize = 53;

pub struct PrettyPrinter<'a> {
    depth: usize,
    line_pos: usize,
    is_tty: bool,
    cur_array_item: Option<usize>,
    indent_size: usize,
    writer: &'a mut (dyn std::io::Write + 'a),
}

impl std::io::Write for PrettyPrinter<'_> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let len = self.writer.write(buf)?;
        let wrote_buf = &buf[..len];
        let wrote_str = String::from_utf8_lossy(wrote_buf);
        self.line_pos = match wrote_buf.iter().rev().position(|b| *b == b'\n') {
            Some(pos) => {
                assert_eq!(pos, 0);
                pos
            }
            None => self.line_pos + wrote_str.chars().count(),
        };
        Ok(len)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.writer.flush()
    }
}

impl<'a> PrettyPrinter<'a> {
    pub fn new(writer: &'a mut (dyn std::io::Write + 'a)) -> Self {
        PrettyPrinter {
            depth: 0,
            line_pos: 0,
            cur_array_item: None,
            is_tty: atty::is(atty::Stream::Stdout),
            indent_size: 2,
            writer,
        }
    }

    pub fn print_table_header(&mut self) -> std::io::Result<()> {
        writeln!(
            self,
            "┌─────────────────────────────────────────────────────────────┬─────────────┐"
        )
    }

    pub fn print_table_footer(&mut self) -> std::io::Result<()> {
        writeln!(
            self,
            "└─────────────────────────────────────────────────────────────┴─────────────┘"
        )
    }

    fn print_indent(&mut self) -> std::io::Result<()> {
        let indent_len = (self.depth * self.indent_size)
            .min(MANY_SPACES.len())
            .saturating_sub(1);
        write!(self, "│")?;
        self.write_all(&MANY_SPACES[..indent_len])
    }

    fn indented(
        &mut self,
        f: impl FnOnce(&mut PrettyPrinter) -> std::io::Result<()>,
    ) -> std::io::Result<()> {
        self.depth += 1;
        let r = f(self);
        self.depth -= 1;
        r
    }

    pub fn print_newline(&mut self) -> std::io::Result<()> {
        writeln!(self)
    }

    pub fn print_root_table<'b>(
        &mut self,
        table: &(dyn SomeTable<'b> + 'b),
    ) -> std::io::Result<()> {
        self.print_table_header()?;
        self.print_indent()?;
        write!(self, "{}", table.type_name())?;
        self.print_hex(&[])?;
        self.print_newline()?;
        self.indented(|this| this.print_fields(table))?;
        self.print_newline()?;
        self.print_table_footer()
    }

    fn print_with_style(
        &mut self,
        style: Style,
        f: impl FnOnce(&mut PrettyPrinter) -> std::io::Result<()>,
    ) -> std::io::Result<()> {
        if !self.is_tty {
            f(self)?;
        } else {
            // ansi styles aren't counted for the purpose of width calculations
            let pos = self.line_pos;
            write!(self, "{}", style.prefix())?;
            self.line_pos = pos;
            f(self)?;
            let pos = self.line_pos;
            write!(self, "{}", style.suffix())?;
            self.line_pos = pos;
        }
        Ok(())
    }

    fn print_fields<'b>(&mut self, table: &(dyn SomeTable<'b> + 'b)) -> std::io::Result<()> {
        for (i, field) in table.iter().enumerate() {
            if i != 0 {
                self.print_newline()?;
            }
            self.print_indent()?;
            self.print_with_style(Color::Cyan.into(), |this| write!(this, "{}", field.name))?;
            write!(self, ": ")?;
            self.print_field(&field.value)?;
        }
        Ok(())
    }

    fn print_array<'b>(&mut self, array: &(dyn SomeArray<'b> + 'b)) -> std::io::Result<()> {
        write!(self, "[{}]", array.type_name())?;
        self.print_hex(&[])?;
        self.print_newline()?;
        self.indented(|this| {
            for (i, item) in array.iter().enumerate() {
                this.cur_array_item = Some(i);
                if i != 0 {
                    this.print_newline()?;
                }
                if is_scalar(&item) {
                    this.print_indent()?;
                }
                this.print_field(&item)?;
            }
            Ok(())
        })
    }

    pub fn print_field(&mut self, field: &FieldType<'_>) -> std::io::Result<()> {
        match &field {
            FieldType::Unknown => write!(self, "unknown")?,
            FieldType::I8(val) => write!(self, "{val}")?,
            FieldType::U8(val) => write!(self, "{val}")?,
            FieldType::I16(val) => write!(self, "{val}")?,
            FieldType::U16(val) => write!(self, "{val}")?,
            FieldType::I32(val) => write!(self, "{val}")?,
            FieldType::U32(val) => write!(self, "{val}")?,
            FieldType::I24(val) => write!(self, "{val}")?,
            FieldType::U24(val) => write!(self, "{val}")?,
            FieldType::Tag(val) => write!(self, "{val}")?,
            FieldType::FWord(val) => write!(self, "{val}")?,
            FieldType::UfWord(val) => write!(self, "{val}")?,
            FieldType::MajorMinor(val) => write!(self, "{val}")?,
            FieldType::Version16Dot16(val) => write!(self, "{val}")?,
            FieldType::F2Dot14(val) => write!(self, "{val}")?,
            FieldType::Fixed(val) => write!(self, "{val}")?,
            FieldType::LongDateTime(val) => write!(self, "{val:?}")?,
            FieldType::GlyphId16(val) => self.print_with_style(Color::Yellow.into(), |this| {
                write!(this, "{}", val.to_u16())
            })?,
            FieldType::NameId(val) => write!(self, "{val:?}")?,
            FieldType::ResolvedOffset(ResolvedOffset { offset, target }) => {
                match target {
                    Ok(table) => {
                        // only indent if we're on a newline, which means we're
                        // in an array
                        if self.line_pos == 0 {
                            self.print_indent()?;
                        }
                        self.print_with_style(Color::Blue.into(), |this| write!(this, "{offset}"))?;
                        self.print_current_array_pos()?;
                        self.print_offset_hex(*offset)?;
                        self.print_newline()?;
                        self.indented(|this| this.print_fields(table))
                    }
                    Err(e) => write!(self, "Error: '{e}'"),
                }?;
            }
            FieldType::StringOffset(StringOffset { offset, target }) => match target {
                Ok(string) => {
                    self.print_with_style(Color::Blue.into(), |this| write!(this, "{offset}"))?;
                    self.print_current_array_pos()?;
                    self.print_offset_hex(*offset)?;
                    self.print_newline()?;
                    self.indented(|this| this.print_string(string))?;
                }
                Err(e) => write!(self, "Error: '{e}'")?,
            },
            FieldType::ArrayOffset(ArrayOffset { offset, target }) => match target {
                Ok(array) => {
                    self.print_with_style(Color::Blue.into(), |this| write!(this, "{offset}"))?;
                    self.print_current_array_pos()?;
                    self.print_offset_hex(*offset)?;
                    self.print_newline()?;
                    self.indented(|this| this.print_array(array))?;
                }
                Err(e) => write!(self, "Error: '{e}'")?,
            },
            FieldType::BareOffset(offset) => {
                if self.line_pos == 0 {
                    self.print_indent()?;
                }
                self.print_with_style(Color::Blue.into(), |this| match offset.to_u32() {
                    0 => write!(this, "Null"),
                    _ => write!(this, "{offset}"),
                })?;
            }
            FieldType::Record(record) => self.print_fields(record)?,
            FieldType::Array(array) => self.print_array(array)?,
        }

        self.print_current_array_pos()?;

        match &field {
            FieldType::I8(val) => self.print_hex(&val.to_be_bytes())?,
            FieldType::U8(val) => self.print_hex(&val.to_be_bytes())?,
            FieldType::I16(val) => self.print_hex(&val.to_be_bytes())?,
            FieldType::U16(val) => self.print_hex(&val.to_be_bytes())?,
            FieldType::I32(val) => self.print_hex(&val.to_be_bytes())?,
            FieldType::U32(val) => self.print_hex(&val.to_be_bytes())?,
            FieldType::U24(val) => self.print_hex(&val.to_be_bytes())?,
            FieldType::Tag(val) => self.print_hex(&val.to_be_bytes())?,
            FieldType::FWord(val) => self.print_hex(&val.to_be_bytes())?,
            FieldType::UfWord(val) => self.print_hex(&val.to_be_bytes())?,
            FieldType::MajorMinor(val) => self.print_hex(&val.to_be_bytes())?,
            FieldType::Version16Dot16(val) => self.print_hex(&val.to_be_bytes())?,
            FieldType::F2Dot14(val) => self.print_hex(&val.to_be_bytes())?,
            FieldType::Fixed(val) => self.print_hex(&val.to_be_bytes())?,
            FieldType::LongDateTime(val) => self.print_hex(&val.to_be_bytes())?,
            FieldType::GlyphId16(val) => self.print_hex(&val.to_be_bytes())?,
            FieldType::BareOffset(offset) => self.print_offset_hex(*offset)?,
            _ => (),
        }
        Ok(())
    }

    // handles very naive linebreaking
    fn print_string(&mut self, string: &dyn SomeString) -> std::io::Result<()> {
        let mut iter = string.iter_chars();
        let mut buf = Vec::new();

        fn fill_next_word(buf: &mut Vec<u8>, iter: &mut impl Iterator<Item = char>) -> usize {
            buf.clear();
            let mut n_chars = 0;
            for next in iter {
                write!(buf, "{next}").unwrap();
                n_chars += 1;
                if next == ' ' {
                    break;
                }
            }
            n_chars
        }

        self.print_indent()?;

        loop {
            match fill_next_word(&mut buf, &mut iter) {
                0 => {
                    self.print_hex(&[])?;
                    return Ok(());
                }
                len if self.line_pos + len < L_COLUMN_WIDTH => {
                    self.print_with_style(Style::default().italic(), |this| this.write_all(&buf))?;
                }
                _ => {
                    self.print_hex(&[])?;
                    self.print_newline()?;
                    self.print_indent()?;
                    self.print_with_style(Style::default().italic(), |this| this.write_all(&buf))?;
                }
            }
        }
    }

    fn print_offset_hex(&mut self, offset: OffsetType) -> std::io::Result<()> {
        match offset {
            OffsetType::Offset16(val) => self.print_hex(&val.to_be_bytes()),
            OffsetType::Offset24(val) => self.print_hex(&val.to_be_bytes()),
            OffsetType::Offset32(val) => self.print_hex(&val.to_be_bytes()),
        }
    }

    fn print_current_array_pos(&mut self) -> std::io::Result<()> {
        if let Some(idx) = self.cur_array_item.take() {
            let padding = ARRAY_POS_WIDTH.saturating_sub(self.line_pos);
            let wspace = &MANY_SPACES[..padding];
            self.write_all(wspace)?;
            self.print_with_style(Color::Fixed(243).italic(), |this| write!(this, " {idx}"))?;
        }
        Ok(())
    }

    fn print_hex(&mut self, bytes: &[u8]) -> std::io::Result<()> {
        if bytes.len() > 4 {
            let (head, tail) = bytes.split_at(4);
            self.print_hex(head)?;
            self.print_newline()?;
            self.print_indent()?;
            self.print_hex(tail)?;
            return Ok(());
        }
        let padding = L_COLUMN_WIDTH.saturating_sub(self.line_pos);
        let wspace = &MANY_SPACES[..padding];
        self.write_all(wspace)?;
        self.print_with_style(Color::Fixed(250).into(), |this| {
            write!(this, "│")?;
            for b in bytes {
                write!(this, " {b:02X}")?
            }
            Ok(())
        })?;
        let padding = (4 - bytes.len()) * 3;
        let wspace = &MANY_SPACES[..padding];
        self.write_all(wspace)?;
        write!(self, " │")
    }
}

fn is_scalar(field_type: &FieldType) -> bool {
    matches!(
        field_type,
        FieldType::I8(_)
            | FieldType::U8(_)
            | FieldType::I16(_)
            | FieldType::U16(_)
            | FieldType::I32(_)
            | FieldType::U32(_)
            | FieldType::U24(_)
            | FieldType::Tag(_)
            | FieldType::FWord(_)
            | FieldType::UfWord(_)
            | FieldType::MajorMinor(_)
            | FieldType::Version16Dot16(_)
            | FieldType::F2Dot14(_)
            | FieldType::Fixed(_)
            | FieldType::LongDateTime(_)
            | FieldType::GlyphId16(_)
    )
}
