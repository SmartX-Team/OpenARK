pub trait AsPrimitive<T>
where
    Self: Clone,
{
    fn as_(self) -> T;
}

macro_rules! impl_as_primitive {
    ( $( & [ $( $ty:ident ),* ] => $dest:ty as $kind:ident , )* ) => {
        $(
            $(
                impl_as_primitive!( & $ty => $dest as $kind , );
            )*
        )*
    };

    ( & number => $dest:ty as $kind:ident , ) => {
        impl_as_primitive!(
            [
                i8, i16, i32, i64,
                u8, u16, u32, u64,
                f32, f64,
            ] => $dest as $kind ,
        );
    };

    ( & half => $dest:ty as $kind:ident , ) => {
        impl_as_primitive!(
            [
                ::half::bf16, ::half::f16,
            ] => $dest as $kind ,
        );
    };

    ( & object => $dest:ty as $kind:ident , ) => {
        impl_as_primitive!(
            [
                String,
            ] => $dest as $kind ,
        );
    };

    ( $( [ $( $ty:ty , )* ] => $dest:ty as $kind:ident , )* ) => {
        $(
           $(
                impl_as_primitive!( $ty => $dest as $kind , );
           )*
        )*
    };

    ( $ty:ty => $dest:ty as cast , ) => {
        impl AsPrimitive<$dest> for $ty {
            fn as_(self) -> $dest {
                self as $dest
            }
        }
    };

    ( $ty:ty => $dest:ty as into , ) => {
        impl AsPrimitive<$dest> for $ty {
            fn as_(self) -> $dest {
                self.into()
            }
        }
    };

    ( $ty:ty => $dest:ty as ignore , ) => {
        impl AsPrimitive<$dest> for $ty {
            fn as_(self) -> $dest {
                Default::default()
            }
        }
    };
}

impl_as_primitive!(
    &[number] => f64 as cast,
    &[half] => f64 as into,
    &[object] => f64 as ignore,
);
