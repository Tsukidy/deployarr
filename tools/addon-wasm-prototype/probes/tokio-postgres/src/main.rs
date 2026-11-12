#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let (client, connection) = tokio_postgres::connect(&std::env::var("DB_URL")?, tokio_postgres::NoTls).await?;
    tokio::spawn(connection);
    let one: i32 = client.query_one("SELECT 1", &[]).await?.get(0);
    println!("select 1 = {one}");
    let n: i32 = client.query_one("SELECT $1::int4 + 1", &[&41i32]).await?.get(0);
    println!("param = {n}");
    Ok(())
}
