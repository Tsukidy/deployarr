use mysql_async::prelude::*;
use mysql_async::{Opts, OptsBuilder, SslOpts};

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut opts = OptsBuilder::from_opts(Opts::from_url(&std::env::var("DB_URL")?)?);
    if let Ok(pem) = std::env::var("CA_PEM") {
        opts = opts.ssl_opts(SslOpts::default().with_root_certs(vec![pem.into_bytes().into()]));
    }
    let mut conn = mysql_async::Conn::new(opts).await?;
    let one: Option<i64> = conn.query_first("SELECT 1").await?;
    println!("select 1 = {one:?}");
    let n: Option<i64> = conn.exec_first("SELECT CAST(? AS SIGNED) + 1", (41i64,)).await?;
    println!("param = {n:?}");
    let status: Option<(String, String)> = conn.query_first("SHOW SESSION STATUS LIKE 'Ssl_cipher'").await?;
    let cipher = status.map(|(_, v)| v).unwrap_or_default();
    println!("tls = {}", if cipher.is_empty() { "none".to_owned() } else { cipher });
    conn.disconnect().await?;
    Ok(())
}
