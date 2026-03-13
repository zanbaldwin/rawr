mod output;

use std::process::ExitCode;

#[tokio::main]
async fn main() -> Result<ExitCode, ()> {
    Ok(ExitCode::SUCCESS)
}
