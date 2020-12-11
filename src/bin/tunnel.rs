use anyhow::Result;
use clap::{Clap, FromArgMatches, IntoApp};

use simple_vpn::*;

fn main() {
    env_logger::init();

    let matches = <Args as IntoApp>::into_app().get_matches();
    let mut args = <Args as FromArgMatches>::from_arg_matches(&matches);
    if matches.occurrences_of("ip") == 0 && matches.occurrences_of("peer_ip") == 0 {
        let default_server_ip = "192.168.200.1".to_string();
        let default_client_ip = "192.168.200.2".to_string();
        match &args.mode {
            Mode::Server(_) => {
                args.ip = default_server_ip.clone();
                args.peer_ip = default_client_ip.clone();
            }
            Mode::Client(_) => {
                args.ip = default_client_ip.clone();
                args.peer_ip = default_server_ip.clone();
            }
        };
    }

    if let Err(e) = try_run(args) {
        eprintln!("{:?}", e);
    }
}

#[derive(Clap)]
#[clap(version = "0.1")]
struct Args {
    #[clap(short, long, default_value = "not_used")]
    ip: String,
    #[clap(short, long, default_value = "not_used")]
    peer_ip: String,
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

fn try_run(args: Args) -> Result<()> {
    match args.mode {
        Mode::Server(config) => {
            let connector = tcp::ListenConnector::new(&config.listen)?;
            let connector = tls::ServerConnector {
                connector,
                pkcs12_path: "./identity.pfx".into(),
                pkcs12_password: "passw0rd".into(),
            };
            let connector = websocket::ListenConnector::new(
                connector,
                websocket::Authentication {
                    username: "hello".into(),
                    password: "world".into(),
                },
            );
            Endpoint::new(&args.ip, &args.peer_ip, connector)?.run_with_retry();
            panic!("endpoint exits");
        }
        Mode::Client(config) => {
            let connector = tcp::StreamConnector {
                addr: config.server,
            };
            let connector = tls::ClientConnector {
                connector,
                hostname: "www.example.com".into(),
                accept_invalid_certs: true,
            };
            let connector = websocket::ClientConnector::new(
                connector,
                "ws://www.example.com/ws".into(),
                websocket::Authentication {
                    username: "hello".into(),
                    password: "world".into(),
                },
            );
            Endpoint::new(&args.ip, &args.peer_ip, connector)?.run_with_retry();
            panic!("endpoint exits");
        }
    }
}
