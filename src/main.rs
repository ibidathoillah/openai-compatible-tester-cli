mod cli;
mod client;
mod config;
mod mock;
mod report;
mod testsuite;
mod types;
mod util;

#[tokio::main]
async fn main() {
    let code = match cli::run().await {
        Ok(code) => code,
        Err(err) => {
            eprintln!("octest internal error: {err:#}");
            5
        }
    };

    std::process::exit(code);
}
