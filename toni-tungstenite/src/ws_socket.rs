use anyhow::Result;
use futures_util::{stream::SplitStream, SinkExt, StreamExt};
use tokio::net::TcpStream;
use tokio::sync::mpsc;
use tokio_tungstenite::{tungstenite::Message, WebSocketStream};
use toni::async_trait;
use toni::websocket::{SendError, Sender, TrySendError, WsError, WsMessage, WsSocket};

pub struct TungsteniteWsSocket {
    pub(crate) inner: WebSocketStream<TcpStream>,
}

impl TungsteniteWsSocket {
    pub fn new(stream: WebSocketStream<TcpStream>) -> Self {
        Self { inner: stream }
    }
}

#[async_trait]
impl WsSocket for TungsteniteWsSocket {
    async fn recv(&mut self) -> Option<Result<WsMessage, WsError>> {
        loop {
            return match self.inner.next().await? {
                Ok(Message::Text(t)) => Some(Ok(WsMessage::Text(t.to_string()))),
                Ok(Message::Binary(b)) => Some(Ok(WsMessage::Binary(b.to_vec()))),
                Ok(Message::Close(_)) => None,
                Ok(Message::Ping(_)) | Ok(Message::Pong(_)) | Ok(Message::Frame(_)) => continue,
                Err(e) => Some(Err(WsError::Internal(e.to_string()))),
            };
        }
    }

    async fn send(&mut self, msg: WsMessage) -> Result<(), WsError> {
        let m = ws_message_to_tungstenite(msg)?;
        self.inner
            .send(m)
            .await
            .map_err(|e| WsError::Internal(e.to_string()))
    }

    /// Mirrors `AxumWsSocket::split()`: a tokio mpsc channel bridges concurrent writes
    /// from `ConnectionManager` to the socket's write half.
    fn split(self) -> (Box<dyn WsSocket>, Box<dyn Sender>) {
        let (write, read) = self.inner.split();
        let (tx, mut rx) = mpsc::channel::<WsMessage>(32);

        tokio::spawn(async move {
            let mut write = write;
            while let Some(msg) = rx.recv().await {
                if let Ok(m) = ws_message_to_tungstenite(msg) {
                    if write.send(m).await.is_err() {
                        break;
                    }
                }
            }
        });

        (
            Box::new(TungsteniteReadSocket(read)),
            Box::new(TokioSender::new(tx)),
        )
    }
}

struct TungsteniteReadSocket(SplitStream<WebSocketStream<TcpStream>>);

#[async_trait]
impl WsSocket for TungsteniteReadSocket {
    async fn recv(&mut self) -> Option<Result<WsMessage, WsError>> {
        loop {
            return match self.0.next().await? {
                Ok(Message::Text(t)) => Some(Ok(WsMessage::Text(t.to_string()))),
                Ok(Message::Binary(b)) => Some(Ok(WsMessage::Binary(b.to_vec()))),
                Ok(Message::Close(_)) => None,
                Ok(Message::Ping(_)) | Ok(Message::Pong(_)) | Ok(Message::Frame(_)) => continue,
                Err(e) => Some(Err(WsError::Internal(e.to_string()))),
            };
        }
    }

    async fn send(&mut self, _msg: WsMessage) -> Result<(), WsError> {
        Err(WsError::Internal(
            "Cannot send on read-only socket (use Sender from split)".into(),
        ))
    }
}

pub struct TokioSender {
    inner: mpsc::Sender<WsMessage>,
}

impl TokioSender {
    pub fn new(tx: mpsc::Sender<WsMessage>) -> Self {
        Self { inner: tx }
    }
}

#[async_trait]
impl Sender for TokioSender {
    async fn send(&self, message: WsMessage) -> Result<(), SendError> {
        self.inner.send(message).await.map_err(|_| SendError)
    }

    fn try_send(&self, message: WsMessage) -> Result<(), TrySendError> {
        self.inner.try_send(message).map_err(|e| match e {
            mpsc::error::TrySendError::Full(msg) => TrySendError::Full(msg),
            mpsc::error::TrySendError::Closed(_) => TrySendError::Closed,
        })
    }
}

fn ws_message_to_tungstenite(msg: WsMessage) -> Result<Message, WsError> {
    match msg {
        WsMessage::Text(t) => Ok(Message::Text(t.into())),
        WsMessage::Binary(b) => Ok(Message::Binary(b.into())),
        WsMessage::Ping(d) => Ok(Message::Ping(d.into())),
        WsMessage::Pong(d) => Ok(Message::Pong(d.into())),
        WsMessage::Close => Ok(Message::Close(None)),
    }
}
