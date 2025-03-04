#![cfg_attr(feature = "python", allow(unused))]

use clap::Parser;

#[derive(Debug, Clone, PartialEq, Eq, Parser)]
struct Cli {}

#[cfg(not(feature = "python"))]
#[tokio::main]
async fn main() {
    let _cli = Cli::parse();
    daft_dashboard::rust::launch().await;
}

#[cfg(feature = "python")]
fn main() {
    unreachable!()
}
