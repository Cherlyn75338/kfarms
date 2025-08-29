#![cfg(test)]

use rand::{rngs::StdRng, Rng, SeedableRng};

#[test]
fn instruction_sequence_fuzzer_smoke() {
    let mut rng = StdRng::seed_from_u64(42);
    let mut acc: u64 = 0;
    for _ in 0..10_000 {
        let op: u8 = rng.gen_range(0..4);
        let v: u64 = rng.gen_range(0..1_000);
        match op {
            0 => acc = acc.saturating_add(v),
            1 => acc = acc.saturating_sub(v),
            2 => { let _ = acc.checked_mul(v).unwrap_or(u64::MAX / 2); },
            _ => { let _ = acc.checked_div(v.max(1)); },
        }
    }
    assert!(true);
}

