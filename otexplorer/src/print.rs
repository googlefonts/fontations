//! pretty printing implementation

use std::io::Write;

use read_fonts::traversal::{FieldType, OffsetType, ResolvedOffset, SomeArray, SomeTable};

static MANY_SPACES: [u8; 200] = [0x20; 200];
// width of the left column, which contains the textual representation.
const L_COLUMN_WIDTH: usize = 59;
// position of array indexes, if they are printed
const ARRAY_POS_WIDTH: usize = 50;

pub struct PrettyPrinter<'a> {
    depth: usize,
    line_pos: usize,
    cur_array_item: Option<usize>,
    indent_size: usize,
    writer: &'a mut (dyn std::io::Write + 'a),
}

impl<'a> std::io::Write for PrettyPrinter<'a> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let len = self.writer.write(buf)?;
        let wrote_buf = &buf[..len];
        self.line_pos = match wrote_buf.iter().rev().position(|b| *b == b'\n') {
            Some(pos) => pos,
            None => self.line_pos + len,
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
            indent_size: 2,
            writer,
        }
    }

    fn print_indent(&mut self) -> std::io::Result<()> {
        let indent_len = (self.depth * self.indent_size).min(MANY_SPACES.len());
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
        self.print_indent()?;
        write!(self, "{}", table.type_name())?;
        self.print_newline()?;
        self.indented(|this| this.print_fields(table))?;
        self.print_newline()
    }

    fn print_fields<'b>(&mut self, table: &(dyn SomeTable<'b> + 'b)) -> std::io::Result<()> {
        for (i, field) in table.iter().enumerate() {
            if i != 0 {
                self.print_newline()?;
            }
            self.print_indent()?;
            write!(self, "{}: ", field.name)?;
            self.print_field(&field.typ)?;
        }
        Ok(())
    }

    fn print_array<'b>(&mut self, array: &(dyn SomeArray<'b> + 'b)) -> std::io::Result<()> {
        writeln!(self, "[{}]", array.type_name())?;
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

    pub fn print_field<'b>(&mut self, field: &FieldType<'b>) -> std::io::Result<()> {
        match &field {
            FieldType::I8(val) => write!(self, "{val}")?,
            FieldType::U8(val) => write!(self, "{val}")?,
            FieldType::I16(val) => write!(self, "{val}")?,
            FieldType::U16(val) => write!(self, "{val}")?,
            FieldType::I32(val) => write!(self, "{val}")?,
            FieldType::U32(val) => write!(self, "{val}")?,
            FieldType::U24(val) => write!(self, "{val}")?,
            FieldType::Tag(val) => write!(self, "{val}")?,
            FieldType::FWord(val) => write!(self, "{val}")?,
            FieldType::UfWord(val) => write!(self, "{val}")?,
            FieldType::MajorMinor(val) => write!(self, "{val}")?,
            FieldType::Version16Dot16(val) => write!(self, "{val}")?,
            FieldType::F2Dot14(val) => write!(self, "{val}")?,
            FieldType::Fixed(val) => write!(self, "{val}")?,
            FieldType::LongDateTime(val) => write!(self, "{val:?}")?,
            FieldType::GlyphId(val) => write!(self, "{}", val.to_u16())?,
            FieldType::ResolvedOffset(ResolvedOffset { offset, target }) => {
                match target {
                    Ok(table) => {
                        // only indent if we're on a newline, which means we're
                        // in an array
                        if self.line_pos == 0 {
                            self.print_indent()?;
                        }
                        write!(self, "+{}", offset.to_u32())?;
                        self.print_current_array_pos()?;
                        self.print_offset_hex(*offset)?;
                        self.print_newline()?;
                        self.indented(|this| this.print_fields(table))
                    }
                    Err(e) => write!(self, "Error: '{e}'"),
                }?;
            }
            FieldType::BareOffset(offset) => write!(self, "{}", offset.to_u32())?,
            FieldType::Record(record) => self.print_fields(record)?,
            FieldType::ValueRecord(record) if record.get_field(0).is_none() => {
                self.write_all(b"Null")?
            }
            FieldType::ValueRecord(record) => self.indented(|this| {
                this.print_newline()?;
                this.print_fields(record)
            })?,
            FieldType::Array(array) => self.print_array(array)?,
            FieldType::None => self.write_all(b"None")?,
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
            FieldType::GlyphId(val) => self.print_hex(&val.to_be_bytes())?,
            FieldType::BareOffset(offset) => self.print_offset_hex(*offset)?,
            _ => (),
        }
        Ok(())
    }

    fn print_offset_hex(&mut self, offset: OffsetType) -> std::io::Result<()> {
        match offset {
            OffsetType::None => Ok(()),
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
            write!(self, " {idx}")?;
        }
        Ok(())
    }

    fn print_hex(&mut self, bytes: &[u8]) -> std::io::Result<()> {
        let padding = L_COLUMN_WIDTH.saturating_sub(self.line_pos);
        let wspace = &MANY_SPACES[..padding];
        self.write_all(wspace)?;
        for b in bytes {
            write!(self, " {b:02X}")?
        }
        Ok(())
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
            | FieldType::GlyphId(_)
    )
}
