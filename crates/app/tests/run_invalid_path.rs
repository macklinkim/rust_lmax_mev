//! A-2 failure: `run("/nonexistent/path")` returns
//! `Err(AppError::Config(ConfigError::Io(_)))`. Verifies that `main`'s
//! exit-code path has a typed error to map.

use rust_lmax_mev_app::{run, AppError};
use rust_lmax_mev_config::ConfigError;

#[test]
fn run_returns_error_on_invalid_config_path() {
    let path = std::path::PathBuf::from("does_not_exist_a8f3c2.toml");
    let err = run(&path).expect_err("missing config file must fail");
    assert!(
        matches!(
            err,
            AppError::Config(ConfigError::Parse(_)) | AppError::Config(ConfigError::Io(_))
        ),
        "expected Config(Io|Parse), got {err:?}"
    );
}
