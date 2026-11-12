use sqlx::mysql::{MySqlConnectOptions, MySqlSslMode};
use sqlx::Connection;

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut opts: MySqlConnectOptions = std::env::var("DB_URL")?.parse()?;
    if let Ok(pem) = std::env::var("CA_PEM") {
        opts = opts.ssl_mode(MySqlSslMode::VerifyIdentity).ssl_ca_from_pem(pem.into_bytes());
    }
    let mut conn = sqlx::MySqlConnection::connect_with(&opts).await?;
    let one: i32 = sqlx::query_scalar("SELECT 1").fetch_one(&mut conn).await?;
    println!("select 1 = {one}");
    let n: i64 = sqlx::query_scalar("SELECT CAST(? AS SIGNED) + 1").bind(41i64).fetch_one(&mut conn).await?;
    println!("param = {n}");
    let (_, cipher): (String, String) = sqlx::query_as("SHOW SESSION STATUS LIKE 'Ssl_cipher'")
        .fetch_one(&mut conn)
        .await?;
    println!("tls = {}", if cipher.is_empty() { "none".to_owned() } else { cipher });
    conn.close().await?;
    Ok(())
}
