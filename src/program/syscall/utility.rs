use libafl_bolts::rands::Rand;

#[inline]
pub fn binary<R: Rand>(rand: &mut R) -> bool {
    rand.below(2) == 0
}

#[inline]
pub fn one_of<R: Rand>(rand: &mut R, n: usize) -> bool {
    rand.below(n) == 0
}

#[inline]
pub fn n_out_of<R: Rand>(rand: &mut R, n: usize, total: usize) -> bool {
    debug_assert!(0 < n && n < total);
    rand.below(total) < n
}
