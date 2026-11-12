use wasi_pg_client::{Config, Connection};

wasip3::cli::command::export!(App);

struct App;

impl wasip3::exports::cli::run::Guest for App {
    async fn run() -> Result<(), ()> {
        run().await.map_err(|error| eprintln!("probe failed: {error}"))
    }
}

async fn run() -> Result<(), Box<dyn std::error::Error>> {
    let config = Config::from_uri(&std::env::var("DB_URL")?)?;
    let mut conn = Connection::connect(&config).await?;
    let result = conn.query("SELECT 1").await?;
    let one: i32 = result.iter().next().ok_or("no row")?.get(0)?;
    println!("select 1 = {one}");
    conn.close().await?;
    Ok(())
}
