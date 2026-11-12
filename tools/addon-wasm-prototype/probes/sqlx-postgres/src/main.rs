use sqlx::postgres::{PgConnectOptions, PgSslMode};
use sqlx::Connection;

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut opts: PgConnectOptions = std::env::var("DB_URL")?.parse()?;
    if let Ok(pem) = std::env::var("CA_PEM") {
        opts = opts.ssl_mode(PgSslMode::VerifyFull).ssl_root_cert_from_pem(pem.into_bytes());
    }
    let mut conn = sqlx::PgConnection::connect_with(&opts).await?;
    let one: i32 = sqlx::query_scalar("SELECT 1").fetch_one(&mut conn).await?;
    println!("select 1 = {one}");
    let n: i32 = sqlx::query_scalar("SELECT $1::int4 + 1").bind(41i32).fetch_one(&mut conn).await?;
    println!("param = {n}");
    let ssl: bool = sqlx::query_scalar("SELECT ssl FROM pg_stat_ssl WHERE pid = pg_backend_pid()")
        .fetch_one(&mut conn)
        .await?;
    println!("tls = {ssl}");
    conn.close().await?;
    Ok(())
}
