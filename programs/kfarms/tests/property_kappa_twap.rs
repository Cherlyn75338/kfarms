#![cfg(test)]

use proptest::prelude::*;

proptest! {
    #[test]
    fn ema_step_is_bounded(delta in 1u64..1_000_000u64, alpha_bps in 1u64..=10_000u64) {
        let step = (delta as u128 * alpha_bps as u128) / 10_000u128;
        prop_assert!(step as u64 <= delta);
    }
}

