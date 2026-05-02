//! Phase 1 binary entrypoint for the LMAX-style MEV engine.
//!
//! Single positional argument: the config TOML path. All wiring lives
//! in [`rust_lmax_mev_app::run`]; this `main` is a thin shell that maps
//! errors to a non-zero exit code and prints them to stderr.

use std::process::ExitCode;

fn main() -> ExitCode {
    let mut args = std::env::args().skip(1);
    let Some(config_path) = args.next() else {
        eprintln!("usage: rust-lmax-mev-app <config.toml>");
        return ExitCode::from(2);
    };
    if args.next().is_some() {
        eprintln!("usage: rust-lmax-mev-app <config.toml>");
        return ExitCode::from(2);
    }

    match rust_lmax_mev_app::run(&config_path) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("rust-lmax-mev-app: {e}");
            ExitCode::FAILURE
        }
    }
}
