//! Support for applying embedded hinting instructions.

use super::{
    cff, Hinting, LocationRef, NormalizedCoord, Outline, OutlineCollection, OutlineCollectionKind,
    OutlineKind, Pen, ScaleError, ScalerMemory, ScalerMetrics, Size,
};

/// Hinter that uses information embedded in the font to perform grid-fitting.
#[derive(Clone)]
pub struct NativeHinter {
    size: Size,
    coords: Vec<NormalizedCoord>,
    hinting: Hinting,
    kind: HinterKind,
}

impl NativeHinter {
    /// Creates a new native hinter for the given outline collection, size,
    /// location in variation space and hinting mode.
    pub fn new<'a>(
        outlines: &OutlineCollection,
        size: Size,
        location: impl Into<LocationRef<'a>>,
        hinting: Hinting,
    ) -> Result<Self, ScaleError> {
        let mut hinter = Self {
            size: Size::unscaled(),
            coords: vec![],
            hinting: Hinting::None,
            kind: HinterKind::None,
        };
        hinter.reconfigure(outlines, size, location, hinting)?;
        Ok(hinter)
    }

    /// Returns the currently configured size.
    pub fn size(&self) -> Size {
        self.size
    }

    /// Returns the currently configured normalized location in variation space.
    pub fn location(&self) -> LocationRef {
        LocationRef::new(&self.coords)
    }

    /// Returns the currently configured hinting mode.
    pub fn hinting(&self) -> Hinting {
        self.hinting
    }

    /// Resets the hinter state for a new font instance with the given
    /// outline collection and settings.
    pub fn reconfigure<'a>(
        &mut self,
        outlines: &OutlineCollection,
        size: Size,
        location: impl Into<LocationRef<'a>>,
        hinting: Hinting,
    ) -> Result<(), ScaleError> {
        self.size = size;
        self.coords.clear();
        self.coords.extend_from_slice(location.into().coords());
        self.hinting = hinting;
        // Reuse memory if the font contains the same outline format
        let current_kind = core::mem::replace(&mut self.kind, HinterKind::None);
        match &outlines.kind {
            OutlineCollectionKind::Glyf(_) => {
                self.kind = HinterKind::Glyf();
            }
            OutlineCollectionKind::Cff(cff) => {
                let mut subfonts = match current_kind {
                    HinterKind::Cff(subfonts) => subfonts,
                    _ => vec![],
                };
                subfonts.clear();
                let size = size.ppem().unwrap_or_default();
                for i in 0..cff.subfont_count() {
                    subfonts.push(cff.subfont(i, size, &self.coords)?);
                }
                self.kind = HinterKind::Cff(subfonts);
            }
            OutlineCollectionKind::None => {}
        }
        Ok(())
    }

    /// Scales and hints the outline and emits the path commands to the given
    /// pen.
    pub fn scale(
        &self,
        outline: &Outline,
        mut memory: ScalerMemory,
        pen: &mut impl Pen,
    ) -> Result<ScalerMetrics, ScaleError> {
        let ppem = self.size.ppem().unwrap_or_default();
        let coords = self.coords.as_slice();
        let hinting = self.hinting;
        match (&self.kind, &outline.kind) {
            (HinterKind::Glyf(..), OutlineKind::Glyf(glyf, outline)) => {
                memory.with_glyf_memory(outline, hinting, |buf| {
                    let mem = outline
                        .memory_from_buffer(buf, hinting)
                        .ok_or(ScaleError::InsufficientMemory)?;
                    let scaled_outline = glyf.scale(mem, outline, ppem, coords)?;
                    scaled_outline.to_path(pen)?;
                    Ok(ScalerMetrics {
                        has_overlaps: outline.has_overlaps,
                        ..Default::default()
                    })
                })
            }
            (HinterKind::Cff(subfonts), OutlineKind::Cff(cff, glyph_id, subfont_ix)) => {
                let Some(subfont) = subfonts.get(*subfont_ix as usize) else {
                    return Err(ScaleError::NoSources);
                };
                cff.outline(
                    subfont,
                    *glyph_id,
                    &self.coords,
                    self.hinting != Hinting::None,
                    pen,
                )?;
                Ok(ScalerMetrics::default())
            }
            _ => Err(ScaleError::NoSources),
        }
    }
}

#[derive(Clone)]
enum HinterKind {
    None,
    Glyf(),
    Cff(Vec<cff::Subfont>),
}
