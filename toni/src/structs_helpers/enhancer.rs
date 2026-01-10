use std::sync::Arc;

use crate::traits_helpers::{ErrorHandler, Guard, Interceptor, Pipe};

pub struct EnhancerMetadata {
    pub guards: Vec<Arc<dyn Guard>>,
    pub pipes: Vec<Arc<dyn Pipe>>,
    pub interceptors: Vec<Arc<dyn Interceptor>>,
    pub error_handlers: Vec<Arc<dyn ErrorHandler>>,
}
