/// parsing-ex
/// Examples that are parsing edge cases

/*
    Example from rand::distributions::distribution.rs
*/

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

/*
    Example from syn::expr::multi_index
*/

use proc_macro2::Span;
use syn::Token;

// simpler example
pub fn syn_token_as_fn() {
    Token![.](Span::call_site());
}

// original example
// fn multi_index(e: &mut Expr, dot_token: &mut Token![.], float: LitFloat) -> Result<bool> {
//     let mut float_repr = float.to_string();
//     let trailing_dot = float_repr.ends_with('.');
//     if trailing_dot {
//         float_repr.truncate(float_repr.len() - 1);
//     }
//     for part in float_repr.split('.') {
//         let index = crate::parse_str(part).map_err(|err| Error::new(float.span(), err))?;
//         #[cfg(not(syn_no_const_vec_new))]
//         let base = mem::replace(e, Expr::DUMMY);
//         #[cfg(syn_no_const_vec_new)]
//         let base = mem::replace(e, Expr::Verbatim(TokenStream::new()));
//         *e = Expr::Field(ExprField {
//             attrs: Vec::new(),
//             base: Box::new(base),
//             dot_token: Token![.](dot_token.span),
//             member: Member::Unnamed(index),
//         });
//         *dot_token = Token![.](float.span());
//     }
//     Ok(!trailing_dot)
// }
