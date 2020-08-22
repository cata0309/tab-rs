use async_tungstenite::{
    tokio::{connect_async, TokioAdapter},
    WebSocketStream,
};
use futures::Future;
use serde::{de::DeserializeOwned, Serialize};

use tokio::net::TcpStream;

use tungstenite::Message;

pub mod bus;
mod common;
pub mod message;
pub mod resource;
pub mod server;
pub mod service;

pub type WebsocketConnection = WebSocketStream<TokioAdapter<TcpStream>>;

pub async fn connect(url: String) -> Result<WebsocketConnection, tungstenite::Error> {
    let tuple = connect_async(url).await?;
    Ok(tuple.0)
}

pub async fn bind(tcp: TcpStream) -> Result<WebsocketConnection, tungstenite::Error> {
    async_tungstenite::tokio::accept_async(tcp).await
}

pub fn decode<T: DeserializeOwned>(
    message: Result<tungstenite::Message, tungstenite::Error>,
) -> anyhow::Result<T> {
    let message = message?;
    let data = bincode::deserialize::<T>(message.into_data().as_slice())?;
    Ok(data)
}

pub fn encode<T: Serialize>(message: T) -> anyhow::Result<tungstenite::Message> {
    let message = bincode::serialize(&message)?;
    Ok(Message::Binary(message))
}

pub fn encode_or_close<T: Serialize, F: FnOnce(&T) -> bool>(
    message: T,
    close_test: F,
) -> anyhow::Result<tungstenite::Message> {
    if close_test(&message) {
        return Ok(Message::Close(None));
    }

    let message = bincode::serialize(&message)?;
    Ok(Message::Binary(message))
}

pub fn encode_with<T: Serialize>(
    message: T,
) -> impl Future<Output = anyhow::Result<tungstenite::Message>> {
    futures::future::ready(encode(message))
}

pub fn decode_with<T: DeserializeOwned>(
    message: Result<tungstenite::Message, tungstenite::Error>,
) -> impl Future<Output = anyhow::Result<T>> {
    futures::future::ready(decode(message))
}