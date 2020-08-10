use anyhow::Result;
use clap::{Clap, FromArgMatches, IntoApp};

use simple_vpn::*;

fn main() {
    // let args = Args::parse();
    let matches = <Args as IntoApp>::into_app().get_matches();
    let mut args = <Args as FromArgMatches>::from_arg_matches(&matches);
    if matches.occurrences_of("ip") == 0 {
        args.ip = match &args.mode {
            Mode::Server(_) => "192.168.200.1".to_string(),
            Mode::Client(_) => "192.168.200.2".to_string(),
        };
    }
    if let Err(e) = smol::run(try_run(args)) {
        eprintln!("{:?}", e);
    }
}

#[derive(Clap)]
#[clap(version = "0.1")]
struct Args {
    #[clap(short, long, default_value = "192.168.200.1")]
    ip: String,
    #[clap(subcommand)]
    mode: Mode,
}

#[derive(Clap)]
enum Mode {
    Client(ClientConfig),
    Server(ServerConfig),
}

#[derive(Clap)]
struct ClientConfig {
    #[clap(short, long, default_value = "127.0.0.1:3000")]
    server: String,
}

#[derive(Clap)]
struct ServerConfig {
    #[clap(short, long, default_value = "0.0.0.0:3000")]
    listen: String,
}

async fn try_run(args: Args) -> Result<()> {
    match args.mode {
        Mode::Server(config) => {
            let listener = std::net::TcpListener::bind(&config.listen)?;
            let connector = tcp::ListenConnector { listener };
            let connector = websocket::ListenConnector { connector };
            let connector = retry::RetryConnector::new(connector);
            Endpoint::new(&args.ip, connector).await?.run().await
        }
        Mode::Client(config) => {
            let connector = tcp::StreamConnector {
                addr: config.server,
            };
            let connector = websocket::ClientConnector {
                connector,
                url: "ws://www.example.com/ws".to_string(),
            };
            let connector = retry::RetryConnector::new(connector);
            Endpoint::new(&args.ip, connector).await?.run().await
        }
    }
}
