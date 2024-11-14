use std::num::NonZeroUsize;

use libafl_bolts::{nonzero, rands::Rand};

#[inline]
pub fn binary<R: Rand>(rand: &mut R) -> bool {
    rand.below(nonzero!(2)) == 0
}

#[inline]
pub fn one_of<R: Rand>(rand: &mut R, n: usize) -> bool {
    assert!(0 < n); // nonzero checked here
    rand.below(unsafe { NonZeroUsize::new_unchecked(n) }) == 0
}

#[inline]
pub fn n_out_of<R: Rand>(rand: &mut R, n: usize, total: usize) -> bool {
    assert!(0 < n && n < total); // nonzero checked here
    rand.below(unsafe { NonZeroUsize::new_unchecked(total) }) < n
}
