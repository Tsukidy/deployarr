const USAGE: &str = "Usage: cargo xtask <command>

Commands:
  help    Print this message";

fn main() {
    match std::env::args().nth(1).as_deref() {
        None | Some("help" | "-h" | "--help") => println!("{USAGE}"),
        Some(other) => {
            eprintln!("unknown command: {other}\n\n{USAGE}");
            std::process::exit(2);
        }
    }
}
