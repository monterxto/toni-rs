use crate::common::TestServer;
use futures_util::stream;
use serial_test::serial;
use toni::{
    controller,
    extractors::{BodyStream, Bytes},
    module, post, Body as ToniBody,
};

#[controller("/stream", pub struct StreamingController;)]
impl StreamingController {
    #[post("/echo")]
    async fn echo(&self, Bytes(body): Bytes) -> ToniBody {
        ToniBody::text(String::from_utf8_lossy(&body).into_owned())
    }

    #[post("/bs-size")]
    async fn bs_size(&self, stream: BodyStream) -> ToniBody {
        use futures_util::{pin_mut, StreamExt};
        let s = stream.into_stream();
        pin_mut!(s);
        let mut total = 0usize;
        while let Some(chunk) = s.next().await {
            total += chunk.unwrap().len();
        }
        ToniBody::text(total.to_string())
    }
}

#[module(controllers: [StreamingController], providers: [])]
impl StreamingModule {}

/// The axum adapter streams the request body via UnsyncBoxBody rather than buffering it.
/// This test sends the body as a stream of chunks to verify end-to-end collection works.
#[serial]
#[tokio_localset_test::localset_test]
async fn test_streaming_body_reaches_controller() {
    let server = TestServer::start(StreamingModule::module_definition()).await;

    // Split payload into chunks to exercise the streaming path
    let chunks: Vec<Result<_, std::io::Error>> = vec![
        Ok(bytes::Bytes::from("hello ")),
        Ok(bytes::Bytes::from("streaming ")),
        Ok(bytes::Bytes::from("world")),
    ];
    let body = reqwest::Body::wrap_stream(stream::iter(chunks));

    let resp = server
        .client()
        .post(server.url("/stream/echo"))
        .body(body)
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 200);
    assert_eq!(resp.text().await.unwrap(), "hello streaming world");
}

#[serial]
#[tokio_localset_test::localset_test]
async fn test_body_stream_into_stream() {
    let server = TestServer::start(StreamingModule::module_definition()).await;

    let chunk_size = 4096usize;
    let total = 256 * 1024usize;
    let chunks: Vec<Result<_, std::io::Error>> = (0..total / chunk_size)
        .map(|_| Ok(bytes::Bytes::from(vec![b'z'; chunk_size])))
        .collect();
    let body = reqwest::Body::wrap_stream(stream::iter(chunks));

    let resp = server
        .client()
        .post(server.url("/stream/bs-size"))
        .body(body)
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 200);
    assert_eq!(resp.text().await.unwrap(), total.to_string());
}
