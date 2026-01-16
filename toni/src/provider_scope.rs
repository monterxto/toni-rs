/// Defines the lifecycle scope of a provider in the dependency injection system.
///
/// This determines when and how often provider instances are created:
/// - **Singleton**: Created once at startup, shared across all requests (default, 95% of use cases)
/// - **Request**: Created once per HTTP request, shared within that request only (5% of use cases)
/// - **Transient**: Created every time it's injected, never cached (<1% of use cases)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderScope {
    /// Created once at application startup and reused for all requests.
    /// This is the default and most common scope.
    ///
    /// **Use for:** Services, repositories, utilities, stateless controllers
    ///
    /// **Performance:** Zero allocations per request (just Arc clone)
    ///
    /// # Example
    /// ```ignore
    /// #[injectable]  // Default is Singleton
    /// pub struct AppService {
    ///     config: ConfigService<AppConfig>
    /// }
    /// ```
    Singleton,

    /// Created once per HTTP request and shared within that request.
    /// Destroyed after the request completes.
    ///
    /// **Use for:** Request-specific context, user session data, audit logging
    ///
    /// # Example
    /// ```ignore
    /// #[injectable(scope = "request")]
    /// pub struct RequestContext {
    ///     request_id: String,
    ///     user: Option<User>,
    /// }
    /// ```
    Request,

    /// Created every time it's injected. Never cached.
    /// Each dependent gets a unique instance.
    ///
    /// **Use for:** Non-cacheable operations, stateful one-off services
    ///
    /// # Example
    /// ```ignore
    /// #[injectable(scope = "transient")]
    /// pub struct PasswordHasher {
    ///     salt: Vec<u8>,  // Unique per instance
    /// }
    /// ```
    Transient,
}

impl Default for ProviderScope {
    fn default() -> Self {
        Self::Singleton
    }
}

impl std::fmt::Display for ProviderScope {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Singleton => write!(f, "singleton"),
            Self::Request => write!(f, "request"),
            Self::Transient => write!(f, "transient"),
        }
    }
}

impl std::str::FromStr for ProviderScope {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "singleton" => Ok(Self::Singleton),
            "request" => Ok(Self::Request),
            "transient" => Ok(Self::Transient),
            _ => Err(format!(
                "Invalid scope: '{}'. Must be 'singleton', 'request', or 'transient'",
                s
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_scope() {
        assert_eq!(ProviderScope::default(), ProviderScope::Singleton);
    }

    #[test]
    fn test_from_str() {
        assert_eq!(
            "singleton".parse::<ProviderScope>().unwrap(),
            ProviderScope::Singleton
        );
        assert_eq!(
            "request".parse::<ProviderScope>().unwrap(),
            ProviderScope::Request
        );
        assert_eq!(
            "transient".parse::<ProviderScope>().unwrap(),
            ProviderScope::Transient
        );

        // Case insensitive
        assert_eq!(
            "SINGLETON".parse::<ProviderScope>().unwrap(),
            ProviderScope::Singleton
        );
        assert_eq!(
            "Request".parse::<ProviderScope>().unwrap(),
            ProviderScope::Request
        );
    }

    #[test]
    fn test_invalid_scope() {
        assert!("invalid".parse::<ProviderScope>().is_err());
        assert!("".parse::<ProviderScope>().is_err());
    }

    #[test]
    fn test_display() {
        assert_eq!(ProviderScope::Singleton.to_string(), "singleton");
        assert_eq!(ProviderScope::Request.to_string(), "request");
        assert_eq!(ProviderScope::Transient.to_string(), "transient");
    }
}
