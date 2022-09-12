//! pretty printing implementation

use read_fonts::traversal::{FieldType, OffsetType, ResolvedOffset, SomeArray, SomeTable};

pub struct PrettyPrinter<'a> {
    depth: usize,
    indent_size: usize,
    writer: &'a mut (dyn std::io::Write + 'a),
}

impl<'a> PrettyPrinter<'a> {
    pub fn new(writer: &'a mut (dyn std::io::Write + 'a)) -> Self {
        PrettyPrinter {
            depth: 0,
            indent_size: 2,
            writer,
        }
    }

    fn print_indent(&mut self) -> std::io::Result<()> {
        static MANY_SPACES: [u8; 200] = [0x20; 200];
        let indent_len = (self.depth * self.indent_size).min(MANY_SPACES.len());
        self.writer.write_all(&MANY_SPACES[..indent_len])
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
        self.writer.write_all(b"\n")
    }

    pub fn print_root_table<'b>(
        &mut self,
        table: &(dyn SomeTable<'b> + 'b),
    ) -> std::io::Result<()> {
        self.print_indent()?;
        self.writer.write_all(table.type_name().as_bytes())?;
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
            self.writer.write_all(field.name.as_bytes())?;
            self.writer.write_all(": ".as_bytes())?;
            self.print_field(&field.typ)?;
        }
        Ok(())
    }

    fn print_array<'b>(&mut self, array: &(dyn SomeArray<'b> + 'b)) -> std::io::Result<()> {
        self.writer
            .write_fmt(format_args!("[{}]\n", array.type_name()))?;
        self.indented(|this| {
            for (i, item) in array.iter().enumerate() {
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

    fn print_offset(&mut self, offset: OffsetType) -> std::io::Result<()> {
        let offset = offset.to_u32();
        let hex_width = super::hex_width(offset);
        self.writer
            .write_fmt(format_args!("0x{offset:0hex_width$X}"))
    }

    pub fn print_field<'b>(&mut self, field: &FieldType<'b>) -> std::io::Result<()> {
        match &field {
            FieldType::I8(val) => self.writer.write_fmt(format_args!("{val}"))?,
            FieldType::U8(val) => self.writer.write_fmt(format_args!("{val}"))?,
            FieldType::I16(val) => self.writer.write_fmt(format_args!("{val}"))?,
            FieldType::U16(val) => self.writer.write_fmt(format_args!("{val}"))?,
            FieldType::I32(val) => self.writer.write_fmt(format_args!("{val}"))?,
            FieldType::U32(val) => self.writer.write_fmt(format_args!("{val}"))?,
            FieldType::U24(val) => self.writer.write_fmt(format_args!("{val}"))?,
            FieldType::Tag(val) => self.writer.write_fmt(format_args!("{val}"))?,
            FieldType::FWord(val) => self.writer.write_fmt(format_args!("{val}"))?,
            FieldType::UfWord(val) => self.writer.write_fmt(format_args!("{val}"))?,
            FieldType::MajorMinor(val) => self.writer.write_fmt(format_args!("{val}"))?,
            FieldType::Version16Dot16(val) => self.writer.write_fmt(format_args!("{val}"))?,
            FieldType::F2Dot14(val) => self.writer.write_fmt(format_args!("{val}"))?,
            FieldType::Fixed(val) => self.writer.write_fmt(format_args!("{val}"))?,
            FieldType::LongDateTime(val) => self.writer.write_fmt(format_args!("{val:?}"))?,
            FieldType::GlyphId(val) => self.writer.write_fmt(format_args!("{}", val.to_u16()))?,
            FieldType::ResolvedOffset(ResolvedOffset { offset, target }) => {
                self.print_offset(*offset)?;
                self.print_newline()?;
                match target {
                    Ok(table) => self.indented(|this| this.print_fields(table)),
                    Err(e) => self.writer.write_fmt(format_args!("Error: '{e}'")),
                }?;
            }
            FieldType::BareOffset(offset) => self.print_offset(*offset)?,
            FieldType::Record(record) => self.print_fields(record)?,
            FieldType::ValueRecord(record) if record.get_field(0).is_none() => {
                self.writer.write_all(b"Null")?
            }
            FieldType::ValueRecord(record) => self.indented(|this| {
                this.print_newline()?;
                this.print_fields(record)
            })?,
            FieldType::Array(array) => self.print_array(array)?,
            FieldType::None => self.writer.write_all(b"None")?,
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
