use bytes::Bytes;
use http_body_util::BodyExt;

use super::FromRequest;
use crate::http_helpers::{HttpRequest, RequestBody, RequestBoxBody};

/// Extracts the request body as a raw, unbuffered stream.
///
/// Use this when you need to process large uploads without loading the entire
/// body into memory. Only one body extractor may appear per handler — the body
/// is single-use.
///
/// # Example
///
/// ```rust,ignore
/// use toni::BodyStream;
/// use futures::StreamExt;
///
/// #[post("/upload")]
/// async fn upload(&self, stream: BodyStream) -> ToniBody {
///     let mut total = 0usize;
///     let mut s = stream.into_stream();
///     while let Some(chunk) = s.next().await {
///         total += chunk.unwrap().len();
///     }
///     ToniBody::text(format!("received {} bytes", total))
/// }
/// ```
pub struct BodyStream(pub(crate) RequestBoxBody);

impl BodyStream {
    /// Consume into a [`futures::Stream`] of `Bytes` chunks.
    pub fn into_stream(
        self,
    ) -> impl futures::Stream<Item = Result<Bytes, Box<dyn std::error::Error + Send + Sync>>> {
        use futures::StreamExt;
        futures::stream::unfold(self.0, |mut body| async move {
            match body.frame().await {
                Some(Ok(frame)) => {
                    if let Ok(data) = frame.into_data() {
                        Some((Ok(data), body))
                    } else {
                        // trailers frame — not data, signal end
                        None
                    }
                }
                Some(Err(e)) => Some((Err(e), body)),
                None => None,
            }
        })
    }

    /// Buffer the entire stream into [`Bytes`].
    pub async fn collect(self) -> Result<Bytes, Box<dyn std::error::Error + Send + Sync>> {
        let collected = self.0.collect().await?;
        Ok(collected.to_bytes())
    }
}

#[derive(Debug)]
pub struct BodyStreamError;

impl std::fmt::Display for BodyStreamError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "missing request body")
    }
}

impl std::error::Error for BodyStreamError {}

impl FromRequest for BodyStream {
    type Error = BodyStreamError;

    async fn from_request(req: HttpRequest) -> Result<Self, Self::Error> {
        let (_, body) = req.into_parts();
        match body {
            RequestBody::Streaming(s) => Ok(BodyStream(s)),
            RequestBody::Buffered(b) => {
                use http_body_util::{Full, BodyExt as _};
                let box_body = Full::new(b)
                    .map_err(|never: std::convert::Infallible| match never {})
                    .boxed_unsync();
                Ok(BodyStream(box_body))
            }
        }
    }
}
