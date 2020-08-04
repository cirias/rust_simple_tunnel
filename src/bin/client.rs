use anyhow::Result;
use clap::Clap;

use simple_vpn::*;

fn main() {
    let args = Args::parse();
    smol::run(try_run(args)).unwrap();
}

#[derive(Clap)]
#[clap(version = "0.1")]
struct Args {
    #[clap(short, long, default_value = "192.168.200.2")]
    ip: String,
    #[clap(short, long, default_value = "127.0.0.1:3000")]
    server: String,
}

async fn try_run(args: Args) -> Result<()> {
    let connector = tcp::StreamConnector { addr: args.server };

    let endpoint = Endpoint::new(&args.ip, connector).await?;

    endpoint.run().await?;

    Ok(())
}
