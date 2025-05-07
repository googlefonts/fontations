#![parse_module(read_fonts::tables::trak)]

/// The [tracking (trak)](https://developer.apple.com/fonts/TrueType-Reference-Manual/RM06/Chap6trak.html) table.
#[tag = "trak"]
table Trak {
    /// Version number of the tracking table (0x00010000 for the current version).
    #[compile(MajorMinor::VERSION_1_0)]
    version: MajorMinor,
    /// Format of the tracking table (set to 0).
    #[compile(0)]
    format: u16,
    /// Offset from start of tracking table to TrackData for horizontal text (or 0 if none).
    #[nullable]
    horiz_offset: Offset16<TrackData>,
    /// Offset from start of tracking table to TrackData for vertical text (or 0 if none).
    #[nullable]
    vert_offset: Offset16<TrackData>,
    /// Reserved. Set to 0.
    #[skip_getter]
    #[compile(0)]
    reserved: u16,
}

/// The tracking data table.
table TrackData {
    /// Number of separate tracks included in this table.
    n_tracks: u16,
    /// Number of point sizes included in this table.
    n_sizes: u16,
    /// Offset from the start of the tracking table to the start of the size subtable.
    size_table_offset: u32,
    /// Array of TrackTableEntry records.
    #[count($n_tracks)]
    track_table: [TrackTableEntry],
}

/// Single entry in a tracking table.
record TrackTableEntry {
    /// Track value for this record.
    track: Fixed,
    /// The 'name' table index for this track (a short word or phrase like "loose" or "very tight"). NameIndex has a value greater than 255 and less than 32768.
    name_index: NameId,
    /// Offset from the start of the tracking table to per-size tracking values for this track.
    offset: u16,
}
