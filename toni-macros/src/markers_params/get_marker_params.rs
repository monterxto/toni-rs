use syn::{ImplItemFn, LitStr, Pat, Result, Type};

use crate::markers_params::remove_marker_controller_fn::is_marker;

pub struct MarkerParam {
    pub param_name: syn::Ident,
    pub param_type: Type,
    pub marker_name: String,
    pub marker_arg: Option<String>,
    pub default_value: Option<String>,
}

pub fn get_marker_params(method: &ImplItemFn) -> Result<Vec<MarkerParam>> {
    let mut marked_params = Vec::new();
    for input in method.sig.inputs.iter() {
        if let syn::FnArg::Typed(pat_type) = input {
            if !pat_type.attrs.is_empty() {
                if let Some(marker_ident) = pat_type.attrs[0].path().get_ident() {
                    if is_marker(marker_ident) {
                        let mut marker_arg = None;
                        let mut default_value = None;

                        if marker_ident.to_string() == "query"
                            || marker_ident.to_string() == "param"
                        {
                            // Parse attribute args: #[query("name")] or #[query("name", default = "10")]
                            use syn::parse::{Parse, ParseStream};
                            use syn::{Ident as SynIdent, Token};

                            struct MarkerArgs {
                                name: Option<String>,
                                default: Option<String>,
                            }

                            impl Parse for MarkerArgs {
                                fn parse(input: ParseStream) -> Result<Self> {
                                    let mut name = None;
                                    let mut default = None;

                                    // Try to parse first argument (parameter name)
                                    if let Ok(lit) = input.parse::<LitStr>() {
                                        name = Some(lit.value());

                                        // Check for comma and default argument
                                        if input.parse::<Token![,]>().is_ok() {
                                            if let Ok(ident) = input.parse::<SynIdent>() {
                                                if ident == "default" {
                                                    input.parse::<Token![=]>()?;
                                                    let default_lit = input.parse::<LitStr>()?;
                                                    default = Some(default_lit.value());
                                                }
                                            }
                                        }
                                    }

                                    Ok(MarkerArgs { name, default })
                                }
                            }

                            if let Ok(args) = pat_type.attrs[0].parse_args::<MarkerArgs>() {
                                marker_arg = args.name;
                                default_value = args.default;
                            }
                        }

                        if let Pat::Ident(pat_ident) = &*pat_type.pat {
                            marked_params.push(MarkerParam {
                                param_name: pat_ident.ident.clone(),
                                param_type: (*pat_type.ty).clone(),
                                marker_name: marker_ident.to_string(),
                                marker_arg,
                                default_value,
                            });
                        }
                    }
                }
            }
        }
    }
    Ok(marked_params)
}
