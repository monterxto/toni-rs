use syn::ItemStruct;

/// Detect which enhancer markers are present on a struct
#[derive(Default, Debug)]
pub struct EnhancerMarkers {
    pub is_guard: bool,
    pub is_interceptor: bool,
    pub is_middleware: bool,
    pub is_pipe: bool,
    pub is_error_handler: bool,
}

impl EnhancerMarkers {
    /// Check struct attributes for enhancer markers
    /// Looks for #[guard], #[interceptor], #[middleware], #[pipe], #[error_handler] attributes
    pub fn detect(struct_attrs: &ItemStruct) -> Self {
        let mut markers = Self::default();

        for attr in &struct_attrs.attrs {
            if let Some(ident) = attr.path().get_ident() {
                let attr_name = ident.to_string();
                match attr_name.as_str() {
                    "guard" => markers.is_guard = true,
                    "interceptor" => markers.is_interceptor = true,
                    "middleware" => markers.is_middleware = true,
                    "pipe" => markers.is_pipe = true,
                    "error_handler" => markers.is_error_handler = true,
                    _ => {}
                }
            }
        }

        markers
    }

    /// Check if any enhancer markers are present
    pub fn has_any(&self) -> bool {
        self.is_guard
            || self.is_interceptor
            || self.is_middleware
            || self.is_pipe
            || self.is_error_handler
    }
}
