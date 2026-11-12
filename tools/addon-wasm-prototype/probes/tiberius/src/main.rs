use tiberius::{Client, Config};
use tokio::net::TcpStream;
use tokio_util::compat::TokioAsyncWriteCompatExt;

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = Config::from_ado_string(&std::env::var("DB_URL")?)?;
    let tcp = TcpStream::connect(config.get_addr()).await?;
    let mut client = Client::connect(config, tcp.compat_write()).await?;
    let one: Option<i32> = client.query("SELECT 1", &[]).await?.into_row().await?.and_then(|r| r.get(0));
    println!("select 1 = {one:?}");
    let n: Option<i32> = client.query("SELECT @P1 + 1", &[&41i32]).await?.into_row().await?.and_then(|r| r.get(0));
    println!("param = {n:?}");
    Ok(())
}
