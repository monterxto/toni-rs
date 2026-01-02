use std::sync::Arc;

use anyhow::Result;

use crate::http_helpers::HttpMethod;
use crate::injector::InstanceWrapper;

pub trait HttpAdapter: Clone + Send + Sync {
    fn add_route(&mut self, path: &str, method: HttpMethod, handler: Arc<InstanceWrapper>);
    fn listen(self, port: u16, hostname: &str) -> impl Future<Output = Result<()>> + Send;
}
