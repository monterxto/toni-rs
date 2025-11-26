/// Trait for types that can be converted into DI container tokens
pub trait IntoToken {
    fn into_token(self) -> String;
}

impl IntoToken for &str {
    fn into_token(self) -> String {
        self.to_string()
    }
}

impl IntoToken for String {
    fn into_token(self) -> String {
        self
    }
}

/// Marker type for type-based tokens
pub struct TypeToken<T: 'static>(std::marker::PhantomData<T>);

impl<T: 'static> TypeToken<T> {
    pub fn new() -> Self {
        Self(std::marker::PhantomData)
    }
}

impl<T: 'static> IntoToken for TypeToken<T> {
    fn into_token(self) -> String {
        std::any::type_name::<T>().to_string()
    }
}

impl<T: 'static> Default for TypeToken<T> {
    fn default() -> Self {
        Self::new()
    }
}
