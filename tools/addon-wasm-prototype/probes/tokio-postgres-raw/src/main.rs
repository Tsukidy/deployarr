use std::future::Future;
use std::io;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

use rustls::pki_types::pem::PemObject;
use rustls::pki_types::{CertificateDer, ServerName};
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio::net::TcpStream;
use tokio_postgres::tls::{ChannelBinding, NoTls, TlsConnect};

struct RustlsConnect {
    config: Arc<rustls::ClientConfig>,
    name: ServerName<'static>,
}

struct RustlsStream(tokio_rustls::client::TlsStream<TcpStream>);

impl TlsConnect<TcpStream> for RustlsConnect {
    type Stream = RustlsStream;
    type Error = io::Error;
    type Future = Pin<Box<dyn Future<Output = io::Result<RustlsStream>> + Send>>;

    fn connect(self, stream: TcpStream) -> Self::Future {
        let connector = tokio_rustls::TlsConnector::from(self.config);
        Box::pin(async move { Ok(RustlsStream(connector.connect(self.name, stream).await?)) })
    }
}

impl tokio_postgres::tls::TlsStream for RustlsStream {
    fn channel_binding(&self) -> ChannelBinding {
        ChannelBinding::none()
    }
}

impl AsyncRead for RustlsStream {
    fn poll_read(mut self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &mut ReadBuf<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.0).poll_read(cx, buf)
    }
}

impl AsyncWrite for RustlsStream {
    fn poll_write(mut self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &[u8]) -> Poll<io::Result<usize>> {
        Pin::new(&mut self.0).poll_write(cx, buf)
    }
    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.0).poll_flush(cx)
    }
    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.0).poll_shutdown(cx)
    }
}

async fn queries(client: tokio_postgres::Client) -> Result<(), tokio_postgres::Error> {
    let one: i32 = client.query_one("SELECT 1", &[]).await?.get(0);
    println!("select 1 = {one}");
    let n: i32 = client.query_one("SELECT $1::int4 + 1", &[&41i32]).await?.get(0);
    println!("param = {n}");
    let ssl: bool = client
        .query_one("SELECT ssl FROM pg_stat_ssl WHERE pid = pg_backend_pid()", &[])
        .await?
        .get(0);
    println!("tls = {ssl}");
    Ok(())
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config: tokio_postgres::Config = std::env::var("DB_URL")?.parse()?;
    let addr = std::env::var("DB_ADDR")?;
    let tcp = TcpStream::connect(&addr).await?;
    match std::env::var("CA_PEM") {
        Ok(pem) => {
            let mut roots = rustls::RootCertStore::empty();
            for cert in CertificateDer::pem_slice_iter(pem.as_bytes()) {
                roots.add(cert?)?;
            }
            let tls = rustls::ClientConfig::builder_with_provider(Arc::new(rustls::crypto::ring::default_provider()))
                .with_safe_default_protocol_versions()?
                .with_root_certificates(roots)
                .with_no_client_auth();
            let host = addr.rsplit_once(':').map(|(h, _)| h).unwrap_or(&addr);
            let connect = RustlsConnect {
                config: Arc::new(tls),
                name: ServerName::try_from(host.to_owned())?,
            };
            let (client, connection) = config.connect_raw(tcp, connect).await?;
            tokio::spawn(connection);
            queries(client).await?;
        }
        Err(_) => {
            let (client, connection) = config.connect_raw(tcp, NoTls).await?;
            tokio::spawn(connection);
            queries(client).await?;
        }
    }
    Ok(())
}
