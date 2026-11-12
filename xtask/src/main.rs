mod winpe_imports;

use std::path::PathBuf;

const USAGE: &str = "Usage: cargo xtask <command>

Commands:
  winpe-imports <exe>  Fail if the executable imports a DLL that base WinPE lacks
  help                 Print this message";

fn main() {
    let mut args = std::env::args().skip(1);
    match args.next().as_deref() {
        None | Some("help" | "-h" | "--help") => println!("{USAGE}"),
        Some("winpe-imports") => {
            let (Some(exe), None) = (args.next(), args.next()) else {
                eprintln!("winpe-imports takes one argument, the executable\n\n{USAGE}");
                std::process::exit(2);
            };
            if let Err(message) = winpe_imports::run(&PathBuf::from(exe)) {
                eprintln!("{message}");
                std::process::exit(1);
            }
        }
        Some(other) => {
            eprintln!("unknown command: {other}\n\n{USAGE}");
            std::process::exit(2);
        }
    }
}
