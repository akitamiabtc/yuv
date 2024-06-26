/// Creates a vector of type ids from a list of types.
#[macro_export]
macro_rules! typeid {
    ($($typ:ty),*) => {
        [$($crate::tid::<$typ>()),*]
    };
}
