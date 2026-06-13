use std::net::TcpStream;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use tungstenite::client::IntoClientRequest;
use tungstenite::handshake::client::Request;
use tungstenite::stream::MaybeTlsStream;
use tungstenite::{connect, WebSocket};

use super::client::AsrError;

const CONNECT_TIMEOUT: Duration = Duration::from_secs(8);

pub fn connect_with_timeout<F>(
    url: &str,
    configure: F,
) -> Result<WebSocket<MaybeTlsStream<TcpStream>>, AsrError>
where
    F: FnOnce(&mut Request) + Send + 'static,
{
    let mut request = url
        .into_client_request()
        .map_err(|err| AsrError::Provider(format!("websocket request: {err}")))?;
    configure(&mut request);

    let (tx, rx) = mpsc::channel();
    thread::Builder::new()
        .name("asr-websocket-connect".into())
        .spawn(move || {
            let result = connect(request)
                .map_err(|err| AsrError::Provider(format!("websocket connect: {err}")))
                .and_then(|(mut socket, _)| {
                    set_socket_nonblocking(socket.get_mut())?;
                    Ok(socket)
                });
            let _ = tx.send(result);
        })
        .map_err(|err| AsrError::Provider(format!("websocket connect thread: {err}")))?;

    rx.recv_timeout(CONNECT_TIMEOUT)
        .map_err(|_| AsrError::Provider("websocket connect timed out".to_string()))?
}

fn set_socket_nonblocking(stream: &mut MaybeTlsStream<TcpStream>) -> Result<(), AsrError> {
    match stream {
        MaybeTlsStream::Plain(stream) => stream
            .set_nonblocking(true)
            .map_err(|err| AsrError::Provider(format!("websocket nonblocking: {err}"))),
        MaybeTlsStream::NativeTls(stream) => stream
            .get_ref()
            .set_nonblocking(true)
            .map_err(|err| AsrError::Provider(format!("websocket nonblocking: {err}"))),
        _ => Err(AsrError::Provider(
            "websocket nonblocking: unsupported tls stream".to_string(),
        )),
    }
}
