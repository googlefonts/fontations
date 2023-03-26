//! The [avar](https://learn.microsoft.com/en-us/typography/opentype/spec/fvar) table

#[path = "./instance_record.rs"]
mod instance_record;

pub use instance_record::InstanceRecord;

include!("../../generated/generated_fvar.rs");
