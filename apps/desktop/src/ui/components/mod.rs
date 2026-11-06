//! Presentational pieces. Each takes plain data and the resolved [`Design`]
//! and knows nothing about `Gallery`, so a region decides what to show and a
//! component only decides how it looks.
//!
//! [`Design`]: super::design::Design

mod detail_list;
mod exposure_pill;
mod exposure_readout;
mod film_chip;

pub(super) use detail_list::DetailList;
pub(super) use exposure_pill::ExposurePill;
pub(super) use exposure_readout::ExposureReadout;
pub(super) use film_chip::FilmChip;
