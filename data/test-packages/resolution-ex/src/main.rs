// Example about canonical paths from https://doc.rust-lang.org/reference/paths.html
// Comments show the canonical path of the item.

mod a {
    // crate::a
    pub struct Struct; // crate::a::Struct

    pub trait Trait {
        // crate::a::Trait
        fn f(&self); // crate::a::Trait::f
    }

    impl Trait for Struct {
        fn f(&self) {} // <crate::a::Struct as crate::a::Trait>::f
    }
    impl Struct {
        fn g(&self) {} // <crate::a::Struct>::g
    }
}

mod without {
    // crate::without
    pub fn canonicals() {
        // crate::without::canonicals
        struct OtherStruct; // None

        trait OtherTrait {
            // None
            fn g(&self); // None
        }

        impl OtherTrait for OtherStruct {
            fn g(&self) {} // None
        }

        impl OtherTrait for crate::a::Struct {
            fn g(&self) {} // None
        }

        impl crate::a::Trait for OtherStruct {
            fn f(&self) {} // None
        }
    }
}

fn main() {
    without::canonicals();
}

mod type_resolution_examples {
    pub struct GenVal<T> {
        pub gen_val: T,
    }

    // impl of GenVal for a generic type `T`
    impl<T> GenVal<T>
    where
        T: ToString,
    {
        pub fn value(&self) -> &T {
            &self.gen_val
        }
    }
    use std::error::Error;
    use std::fmt::{Debug, Display};
    pub trait OtherError: Debug + Display {
        type TraitItem;

        fn source(&self) -> Option<Option<&(dyn Error + 'static)>>;
    }

    fn test_cases() -> Result<(), ()> {
        let mut targets: Vec<i32> = Vec::new();
        // resolve closures
        let mut _push_new_def = |item: i32| {
            if !targets.contains(&item) {
                targets.push(item);
            }
        };

        let t = |j| println!("hello, {}", j);

        // resolve different data types
        let _some_constructor = Some::<i32>;
        let _push_integer = Vec::<i32>::push;
        let _slice_reverse = <[i32]>::reverse;
        let _expr = ([1, 2, 3, 4])[2];
        let _b = [[1, 0, 0], [0, 1, 0], [0, 0, 1]];
        let _pair = ("a string", 2);
        let _point = _pair.0;
        let _y = 0..10;
        let _x = to_vec(&[1..4]);

        // resolve function params and return types
        let pi: Result<f32, _> = "3.14".parse();
        let _log_pi = pi.unwrap_or(1.0).log(2.72);
        ten_times(t);

        //resolve generic params
        fn ten_times<F>(f: F)
        where
            F: Fn(i32),
        {
            for index in 0..10 {
                f(index);
            }
        }

        fn to_vec<A: Clone>(xs: &[A]) -> Vec<A> {
            if xs.is_empty() {
                return vec![];
            }
            let first: A = xs[0].clone();
            let mut rest: Vec<A> = to_vec(&xs[1..]);
            rest.insert(0, first);
            rest
        }

        // resolve field parameters
        let y = GenVal { gen_val: 3 };
        let _v = y.value();

        // resolve type aliases
        type Z = dyn OtherError<TraitItem = u32>;

        Ok(())
    }
}
