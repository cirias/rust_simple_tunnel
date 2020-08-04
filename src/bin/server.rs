use std::net;

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
    #[clap(short, long, default_value = "192.168.200.1")]
    ip: String,
    #[clap(short, long, default_value = "0.0.0.0:3000")]
    listen: String,
}

async fn try_run(args: Args) -> Result<()> {
    let listener = net::TcpListener::bind(&args.listen)?;

    let connector = tcp::ListenConnector { listener };

    let endpoint = Endpoint::new(&args.ip, connector).await?;

    endpoint.read_write().await?;

    Ok(())
}
