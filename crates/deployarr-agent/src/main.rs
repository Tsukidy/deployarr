#[cfg(all(
    target_arch = "x86_64",
    target_os = "windows",
    target_env = "msvc",
    not(target_feature = "crt-static")
))]
compile_error!(
    "deployarr-agent must link the static C runtime. A RUSTFLAGS environment variable replaces the flags in .cargo/config.toml, so add -C target-feature=+crt-static to it."
);

#[cfg(all(target_arch = "x86_64", target_os = "windows", target_env = "msvc"))]
fn main() {}

#[cfg(not(all(target_arch = "x86_64", target_os = "windows", target_env = "msvc")))]
fn main() {
    eprintln!("deployarr-agent runs only on x86_64-pc-windows-msvc");
    std::process::exit(1);
}
