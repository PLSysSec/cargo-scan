// Weird example from serde_json features_check with a badly formed file
// that is used to produce a compiler error

// Shows a user-friendly compiler error on incompatible selected features.

#[allow(unused_macros)]
macro_rules! hide_from_rustfmt {
    ($mod:item) => {
        $mod
    };
}

#[cfg(not(any(feature = "std", feature = "alloc")))]
hide_from_rustfmt! {
    mod error;
}