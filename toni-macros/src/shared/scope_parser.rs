use syn::{
    Attribute, ItemStruct, LitStr, Result, Token,
    parse::{Parse, ParseStream},
};

/// Provider scope types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderScope {
    Singleton,
    Request,
    Transient,
}

impl Default for ProviderScope {
    fn default() -> Self {
        Self::Singleton
    }
}

/// Controller scope types (only Singleton and Request)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ControllerScope {
    Singleton,
    Request,
}

impl Default for ControllerScope {
    fn default() -> Self {
        Self::Singleton // Controllers are Singleton by default (like NestJS)
    }
}

/// Parse injectable attribute
/// Supports two syntaxes:
/// 1. Attribute: #[injectable(scope = "request", init = "new")] pub struct Foo { ... }
/// 2. Inline: #[injectable(scope = "request", pub struct Foo { ... })]
pub struct ProviderStructArgs {
    pub scope: ProviderScope,
    pub init: Option<String>, // Optional custom constructor method name
    pub struct_def: Option<ItemStruct>, // None if using attribute syntax
}

/// Parse controller_struct attribute: #[controller_struct(scope = "request", init = "new", pub struct Foo { ... })]
pub struct ControllerStructArgs {
    pub scope: ControllerScope,
    pub was_explicit: bool,   // Did user explicitly write scope = "..."?
    pub init: Option<String>, // Optional custom constructor method name
    pub struct_def: ItemStruct,
}

/// Parse new consolidated controller attribute
/// Supports:
/// - #[controller(pub struct Foo { ... })]
/// - #[controller("/path", pub struct Foo { ... })]
/// - #[controller("/path", scope = "request", pub struct Foo { ... })]
pub struct ControllerArgs {
    pub path: String, // Controller path prefix (empty string if not specified)
    pub scope: ControllerScope,
    pub was_explicit: bool,     // Did user explicitly write scope = "..."?
    pub init: Option<String>,   // Optional custom constructor method name
    pub struct_def: ItemStruct, // The struct definition
}

impl Parse for ProviderStructArgs {
    fn parse(input: ParseStream) -> Result<Self> {
        let mut scope = ProviderScope::default();
        let mut init: Option<String> = None;

        // Parse optional attributes: scope = "...", init = "..."
        while input.peek(syn::Ident) && !input.peek(Token![pub]) && !input.peek(Token![struct]) {
            let ident: syn::Ident = input.parse()?;

            if ident == "scope" {
                // Parse: scope = "request"
                let _eq: Token![=] = input.parse()?;
                let value: LitStr = input.parse()?;

                scope = match value.value().as_str() {
                    "singleton" => ProviderScope::Singleton,
                    "request" => ProviderScope::Request,
                    "transient" => ProviderScope::Transient,
                    other => {
                        return Err(syn::Error::new(
                            value.span(),
                            format!(
                                "Invalid scope: '{}'. Must be 'singleton', 'request', or 'transient'",
                                other
                            ),
                        ));
                    }
                };
            } else if ident == "init" {
                // Parse: init = "new"
                let _eq: Token![=] = input.parse()?;
                let value: LitStr = input.parse()?;
                init = Some(value.value());
            } else {
                return Err(syn::Error::new(
                    ident.span(),
                    format!("Unknown attribute: '{}'. Expected 'scope' or 'init'", ident),
                ));
            }

            // Consume the comma after attribute
            if input.peek(Token![,]) {
                let _: Token![,] = input.parse()?;
            }
        }

        // Try to parse struct definition (inline syntax)
        // If input is empty, struct_def will be None (attribute syntax)
        let struct_def = if !input.is_empty() {
            Some(input.parse::<ItemStruct>()?)
        } else {
            None
        };

        Ok(ProviderStructArgs {
            scope,
            init,
            struct_def,
        })
    }
}

impl Parse for ControllerStructArgs {
    fn parse(input: ParseStream) -> Result<Self> {
        let mut scope = ControllerScope::default();
        let mut was_explicit = false;
        let mut init: Option<String> = None;

        // Parse optional attributes: scope = "...", init = "..."
        while input.peek(syn::Ident) && !input.peek(Token![pub]) && !input.peek(Token![struct]) {
            let ident: syn::Ident = input.parse()?;

            if ident == "scope" {
                // Parse: scope = "request"
                let _eq: Token![=] = input.parse()?;
                let value: LitStr = input.parse()?;

                was_explicit = true; // User explicitly set the scope
                scope = match value.value().as_str() {
                    "singleton" => ControllerScope::Singleton,
                    "request" => ControllerScope::Request,
                    other => {
                        return Err(syn::Error::new(
                            value.span(),
                            format!(
                                "Invalid controller scope: '{}'. Must be 'singleton' or 'request'. Note: Controllers cannot be 'transient'",
                                other
                            ),
                        ));
                    }
                };
            } else if ident == "init" {
                // Parse: init = "new"
                let _eq: Token![=] = input.parse()?;
                let value: LitStr = input.parse()?;
                init = Some(value.value());
            } else {
                return Err(syn::Error::new(
                    ident.span(),
                    format!("Unknown attribute: '{}'. Expected 'scope' or 'init'", ident),
                ));
            }

            // Consume the comma after attribute
            if input.peek(Token![,]) {
                let _: Token![,] = input.parse()?;
            }
        }

        // Parse the struct definition
        let struct_def: ItemStruct = input.parse()?;

        Ok(ControllerStructArgs {
            scope,
            was_explicit,
            init,
            struct_def,
        })
    }
}

impl Parse for ControllerArgs {
    fn parse(input: ParseStream) -> Result<Self> {
        let mut path = String::new();
        let mut scope = ControllerScope::default();
        let mut was_explicit = false;
        let mut init: Option<String> = None;

        // Check if first token is a string literal (path)
        if input.peek(LitStr) {
            let path_lit: LitStr = input.parse()?;
            path = path_lit.value();

            // Consume comma if present
            if input.peek(Token![,]) {
                let _: Token![,] = input.parse()?;
            }
        }

        // Parse optional named arguments: scope = "...", init = "..."
        while input.peek(syn::Ident) && !input.peek(Token![pub]) && !input.peek(Token![struct]) {
            let ident: syn::Ident = input.parse()?;

            if ident == "scope" {
                let _eq: Token![=] = input.parse()?;
                let value: LitStr = input.parse()?;

                was_explicit = true;
                scope = match value.value().as_str() {
                    "singleton" => ControllerScope::Singleton,
                    "request" => ControllerScope::Request,
                    other => {
                        return Err(syn::Error::new(
                            value.span(),
                            format!(
                                "Invalid controller scope: '{}'. Must be 'singleton' or 'request'",
                                other
                            ),
                        ));
                    }
                };
            } else if ident == "init" {
                let _eq: Token![=] = input.parse()?;
                let value: LitStr = input.parse()?;
                init = Some(value.value());
            } else {
                return Err(syn::Error::new(
                    ident.span(),
                    format!("Unknown attribute: '{}'. Expected 'scope' or 'init'", ident),
                ));
            }

            // Consume comma if present
            if input.peek(Token![,]) {
                let _: Token![,] = input.parse()?;
            }
        }

        // Parse the struct definition
        let struct_def: ItemStruct = input.parse()?;

        Ok(ControllerArgs {
            path,
            scope,
            was_explicit,
            init,
            struct_def,
        })
    }
}

/// Extract scope from attributes like #[scope("singleton")]
/// DEPRECATED: Use ProviderStructArgs instead
pub fn parse_scope_from_attrs(attrs: &[Attribute]) -> Result<ProviderScope> {
    for attr in attrs {
        if attr.path().is_ident("scope") {
            let value: LitStr = attr.parse_args()?;

            let scope = match value.value().as_str() {
                "singleton" => ProviderScope::Singleton,
                "request" => ProviderScope::Request,
                "transient" => ProviderScope::Transient,
                other => {
                    return Err(syn::Error::new(
                        value.span(),
                        format!(
                            "Invalid scope: '{}'. Must be 'singleton', 'request', or 'transient'",
                            other
                        ),
                    ));
                }
            };

            return Ok(scope);
        }
    }

    // Default to singleton if no scope attribute found
    Ok(ProviderScope::default())
}

#[cfg(test)]
mod tests {
    use super::*;
    use quote::quote;
    use syn::parse_quote;

    #[test]
    fn test_parse_singleton_scope() {
        let attr: Attribute = parse_quote! {
            #[scope("singleton")]
        };
        let scope = parse_scope_from_attrs(&[attr]).unwrap();
        assert_eq!(scope, ProviderScope::Singleton);
    }

    #[test]
    fn test_parse_request_scope() {
        let attr: Attribute = parse_quote! {
            #[scope("request")]
        };
        let scope = parse_scope_from_attrs(&[attr]).unwrap();
        assert_eq!(scope, ProviderScope::Request);
    }

    #[test]
    fn test_parse_transient_scope() {
        let attr: Attribute = parse_quote! {
            #[scope("transient")]
        };
        let scope = parse_scope_from_attrs(&[attr]).unwrap();
        assert_eq!(scope, ProviderScope::Transient);
    }

    #[test]
    fn test_default_scope() {
        // No scope attribute = defaults to singleton
        let scope = parse_scope_from_attrs(&[]).unwrap();
        assert_eq!(scope, ProviderScope::Singleton);
    }

    #[test]
    fn test_invalid_scope() {
        let attr: Attribute = parse_quote! {
            #[scope("invalid")]
        };
        let result = parse_scope_from_attrs(&[attr]);
        assert!(result.is_err());
    }
}
