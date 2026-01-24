use std::sync::Arc;

use anyhow::Result;

use crate::http_helpers::{HttpRequest, HttpResponse, ToResponse};
use crate::injector::InstanceWrapper;

pub trait RouteAdapter {
    type Request;
    type Response;

    fn adapt_request(request: Self::Request) -> impl Future<Output = Result<HttpRequest>>;

    fn adapt_response(
        response: Box<dyn ToResponse<Response = HttpResponse>>,
    ) -> Result<Self::Response>;

    fn handle_request(
        request: Self::Request,
        controller: Arc<InstanceWrapper>,
    ) -> impl Future<Output = Result<Self::Response>> {
        async move {
            let http_request = Self::adapt_request(request).await?;
            let http_response = controller.handle_request(http_request).await;
            Self::adapt_response(http_response)
        }
    }
}
