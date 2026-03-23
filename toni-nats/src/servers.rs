/// Conversion into a list of NATS server URLs.
///
/// Mirrors the ergonomics of `async_nats`'s own `IntoServerAddrs` so callers
/// can pass a single URL string or a collection without needing two different
/// constructors.
///
/// # Implemented for
///
/// - `&str` / `String` — single server
/// - `Vec<&str>` / `Vec<String>` — explicit list
/// - `[&str; N]` / `[String; N]` — inline arrays
pub trait IntoNatsServers: sealed::Sealed {
    fn into_servers(self) -> Vec<String>;
}

impl IntoNatsServers for &str {
    fn into_servers(self) -> Vec<String> {
        vec![self.to_string()]
    }
}

impl IntoNatsServers for String {
    fn into_servers(self) -> Vec<String> {
        vec![self]
    }
}

impl IntoNatsServers for Vec<String> {
    fn into_servers(self) -> Vec<String> {
        self
    }
}

impl IntoNatsServers for Vec<&str> {
    fn into_servers(self) -> Vec<String> {
        self.into_iter().map(|s| s.to_string()).collect()
    }
}

impl<const N: usize> IntoNatsServers for [&str; N] {
    fn into_servers(self) -> Vec<String> {
        self.into_iter().map(|s| s.to_string()).collect()
    }
}

impl<const N: usize> IntoNatsServers for [String; N] {
    fn into_servers(self) -> Vec<String> {
        self.into_iter().collect()
    }
}

mod sealed {
    pub trait Sealed {}
    impl Sealed for &str {}
    impl Sealed for String {}
    impl Sealed for Vec<String> {}
    impl Sealed for Vec<&str> {}
    impl<const N: usize> Sealed for [&str; N] {}
    impl<const N: usize> Sealed for [String; N] {}
}
