use thiserror::Error;
use tokio::net::UdpSocket;

#[derive(Debug, Error)]
pub enum SocketError {
    #[error("io error")]
    Io(#[from] std::io::Error),
}

pub async fn bind_udp(addr: &str) -> Result<UdpSocket, SocketError> {
    Ok(UdpSocket::bind(addr).await?)
}

pub async fn send_udp(socket: &UdpSocket, data: &[u8], to: &str) -> Result<usize, SocketError> {
    Ok(socket.send_to(data, to).await?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn bind_udp_socket() {
        let socket = bind_udp("127.0.0.1:0").await.expect("bind");
        let addr = socket.local_addr().expect("addr");
        assert_eq!(addr.ip().to_string(), "127.0.0.1");
    }
}
