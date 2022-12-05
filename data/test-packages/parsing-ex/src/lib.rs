// from rand::distributions::distribution.rs

use rand::Rng;
use rand::distributions::{Distribution, Uniform};

pub fn test_make_an_iter() {
    fn ten_dice_rolls_other_than_five<R: Rng>(
        rng: &mut R,
    ) -> impl Iterator<Item = i32> + '_ {
        Uniform::new_inclusive(1, 6)
            .sample_iter(rng)
            .filter(|x| *x != 5)
            .take(10)
    }

    let mut rng = rand::thread_rng();
    let mut count = 0;
    for val in ten_dice_rolls_other_than_five(&mut rng) {
        assert!((1..=6).contains(&val) && val != 5);
        count += 1;
    }
    assert_eq!(count, 10);
}

#[test]
fn test_make_an_iter_wrapper() {
    test_make_an_iter();
}
