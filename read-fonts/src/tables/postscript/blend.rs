//! Support for the dictionary and charstring blend operator.

use font_types::{BigEndian, F2Dot14, Fixed};

use super::Error;
use crate::tables::variations::{ItemVariationData, ItemVariationStore, VariationRegionList};

/// The maximum number of region scalars that we precompute.
const MAX_PRECOMPUTED_SCALARS: usize = 16;

/// State for processing the blend operator for DICTs and charstrings.
///
/// To avoid allocation, scalars are computed on demand but this can be
/// prohibitively expensive in charstrings where blends are applied to
/// large numbers of elements. To amortize the cost, a fixed number of
/// precomputed scalars are stored internally and the overflow is computed
/// as needed.
///
/// The `MAX_PRECOMPUTED_SCALARS` constant determines the size of the
/// internal buffer (currently 16).
pub struct BlendState<'a> {
    store: ItemVariationStore<'a>,
    regions: VariationRegionList<'a>,
    coords: &'a [F2Dot14],
    vs_index: u16,
    needs_update: bool,
    // The following need to be updated when `needs_update` is true:
    data: Option<ItemVariationData<'a>>,
    region_indices: &'a [BigEndian<u16>],
    scalars: [Fixed; MAX_PRECOMPUTED_SCALARS],
}

impl<'a> BlendState<'a> {
    pub fn new(
        store: ItemVariationStore<'a>,
        coords: &'a [F2Dot14],
        vs_index: u16,
    ) -> Result<Self, Error> {
        let regions = store.variation_region_list()?;
        Ok(Self {
            store,
            regions,
            coords,
            vs_index,
            needs_update: true,
            data: None,
            region_indices: &[],
            scalars: Default::default(),
        })
    }

    /// Sets the active variation store index.
    ///
    /// This should be called with the operand of the `vsindex` operator
    /// for both DICTs and charstrings.
    pub fn set_vs_index(&mut self, vs_index: u16) {
        if vs_index != self.vs_index {
            self.vs_index = vs_index;
            self.needs_update = true;
        }
    }

    /// Returns the number of variation regions for the currently active
    /// variation store index.
    pub fn region_count(&mut self) -> Result<usize, Error> {
        self.update_if_needed()?;
        Ok(self.region_indices.len())
    }

    /// Returns an iterator yielding scalars for each variation region of
    /// the currently active variation store index.
    pub fn scalars(&mut self) -> Result<impl Iterator<Item = Result<Fixed, Error>> + '_, Error> {
        self.update_if_needed()?;
        let total_count = self.region_indices.len();
        let cached = &self.scalars[..MAX_PRECOMPUTED_SCALARS.min(total_count)];
        let remaining_regions = if total_count > MAX_PRECOMPUTED_SCALARS {
            &self.region_indices[MAX_PRECOMPUTED_SCALARS..]
        } else {
            &[]
        };
        Ok(cached.iter().copied().map(Ok).chain(
            remaining_regions
                .iter()
                .map(|region_ix| self.region_scalar(region_ix.get())),
        ))
    }

    fn update_if_needed(&mut self) -> Result<(), Error> {
        if !self.needs_update {
            return Ok(());
        }
        self.needs_update = false;
        self.data = None;
        self.region_indices = &[];
        let store = &self.store;
        let varation_datas = store.item_variation_datas();
        let data = varation_datas
            .get(self.vs_index as usize)
            .ok_or(Error::InvalidVsIndex(self.vs_index))??;
        let region_indices = data.region_indexes();
        let regions = self.regions.variation_regions();
        // Precompute scalars for all regions up to MAX_PRECOMPUTED_SCALARS
        for (region_ix, scalar) in region_indices
            .iter()
            .take(MAX_PRECOMPUTED_SCALARS)
            .zip(&mut self.scalars)
        {
            // We can't use region_scalar here because self is already borrowed
            // as mutable above
            let region = regions.get(region_ix.get() as usize)?;
            *scalar = region.compute_scalar(self.coords);
        }
        self.data = Some(data);
        self.region_indices = region_indices;
        Ok(())
    }

    fn region_scalar(&self, index: u16) -> Result<Fixed, Error> {
        Ok(self
            .regions
            .variation_regions()
            .get(index as usize)
            .map_err(Error::Read)?
            .compute_scalar(self.coords))
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::{FontData, FontRead};

    #[test]
    fn example_blends() {
        // args are (coords, expected_scalars)
        example_test(&[-1.0], &[0.0, 1.0]);
        example_test(&[-0.25], &[0.5, 0.0]);
        example_test(&[-0.5], &[1.0, 0.0]);
        example_test(&[-0.75], &[0.5, 0.5]);
        example_test(&[0.0], &[0.0, 0.0]);
        example_test(&[0.5], &[0.0, 0.0]);
        example_test(&[1.0], &[0.0, 0.0]);
    }

    fn example_test(coords: &[f32], expected: &[f64]) {
        let scalars = example_scalars_for_coords(coords);
        let expected: Vec<_> = expected.iter().copied().map(Fixed::from_f64).collect();
        assert_eq!(scalars, expected);
    }

    fn example_scalars_for_coords(coords: &[f32]) -> Vec<Fixed> {
        let ivs = example_ivs();
        let coords: Vec<_> = coords
            .iter()
            .map(|coord| F2Dot14::from_f32(*coord))
            .collect();
        let mut blender = BlendState::new(ivs, &coords, 0).unwrap();
        blender.scalars().unwrap().map(|res| res.unwrap()).collect()
    }

    fn example_ivs() -> ItemVariationStore<'static> {
        // ItemVariationStore is at offset 18 in the CFF example font
        let ivs_data = &font_test_data::cff2::EXAMPLE[18..];
        ItemVariationStore::read(FontData::new(ivs_data)).unwrap()
    }
}
