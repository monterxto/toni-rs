use proc_macro2::TokenStream;
use quote::quote;
use syn::{
    LitStr, Path, Result,
    parse::{Parse, ParseStream},
};

/// Represents the three types of tokens supported in the DI system
#[derive(Clone)]
pub enum TokenType {
    String(String), // "API_KEY" - string literal token
    Type(Path),     // IAuthService or my_mod::Type - type-based token
    Const(Path),    // APP_GUARD - const token (SCREAMING_SNAKE_CASE)
}

impl TokenType {
    /// Generate code that evaluates to the token string at runtime
    pub fn to_token_expr(&self) -> TokenStream {
        match self {
            TokenType::String(s) => quote! { #s.to_string() },
            TokenType::Type(path) => quote! {
                std::any::type_name::<#path>().to_string()
            },
            TokenType::Const(path) => {
                // Support both Token<T> consts and string consts
                // Token<T> has .name() method, &str consts can use .to_string()
                // We generate code that works for both by trying Token<T> pattern first
                quote! {
                    {
                        // Try to use as Token<T> first (has .name() method)
                        // If it's a &str const, this will be the fallback
                        let __token_value = #path;

                        // Use trait-based detection at compile time
                        trait __ToniTokenName {
                            fn __get_token_name(&self) -> String;
                        }

                        // Implement for Token<T> - use .name()
                        impl<T: ?Sized> __ToniTokenName for ::toni::di::Token<T> {
                            fn __get_token_name(&self) -> String {
                                self.name().to_string()
                            }
                        }

                        // Implement for &str - use .to_string()
                        impl __ToniTokenName for &str {
                            fn __get_token_name(&self) -> String {
                                self.to_string()
                            }
                        }

                        __token_value.__get_token_name()
                    }
                }
            }
        }
    }

    /// Get display name for error messages
    pub fn display_name(&self) -> String {
        match self {
            TokenType::String(s) => format!("\"{}\"", s),
            TokenType::Type(path) => quote!(#path).to_string(),
            TokenType::Const(path) => quote!(#path).to_string(),
        }
    }
}

impl Parse for TokenType {
    fn parse(input: ParseStream) -> Result<Self> {
        // Try string literal first
        if input.peek(LitStr) {
            let lit_str: LitStr = input.parse()?;
            return Ok(TokenType::String(lit_str.value()));
        }

        // Otherwise parse as path (could be Type or Const)
        let path: Path = input.parse()?;

        // Detect if const (SCREAMING_SNAKE_CASE)
        if is_const_identifier(&path) {
            Ok(TokenType::Const(path))
        } else {
            Ok(TokenType::Type(path))
        }
    }
}

/// Check if a path represents a const identifier (SCREAMING_SNAKE_CASE)
fn is_const_identifier(path: &Path) -> bool {
    if let Some(ident) = path.get_ident() {
        let name = ident.to_string();
        // Must be ALL_CAPS with underscores
        !name.is_empty()
            && name
                .chars()
                .all(|c| c.is_uppercase() || c.is_numeric() || c == '_')
            && name.chars().next().unwrap().is_uppercase()
    } else {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use syn::parse_quote;

    #[test]
    fn test_parse_string_token() {
        let token: TokenType = parse_quote!("API_KEY");
        assert!(matches!(token, TokenType::String(s) if s == "API_KEY"));
    }

    #[test]
    fn test_parse_type_token() {
        let token: TokenType = parse_quote!(AuthService);
        assert!(matches!(token, TokenType::Type(_)));
    }

    #[test]
    fn test_parse_const_token() {
        let token: TokenType = parse_quote!(APP_GUARD);
        assert!(matches!(token, TokenType::Const(_)));
    }
}
