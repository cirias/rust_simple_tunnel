use std::net::ToSocketAddrs;
use std::process::Command;

use anyhow::{anyhow, Result};
use clap::{Clap, FromArgMatches, IntoApp};

use simple_tunnel::*;

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
    #[clap(short, long, default_value = "www.example.com")]
    hostname: String,
    #[clap(short, long)]
    not_accept_invalid_certs: bool,
    #[clap(short, long, default_value = "hello")]
    username: String,
    #[clap(short, long, default_value = "world")]
    password: String,
    #[clap(short, long)]
    script_path: Option<String>,
}

#[derive(Clap)]
struct ServerConfig {
    #[clap(short, long, default_value = "0.0.0.0:3000")]
    listen: String,
    #[clap(short, long, default_value = "./identity.pfx")]
    pkcs12_path: String,
    #[clap(short, long, default_value = "passw0rd")]
    pkcs12_password: String,
    #[clap(short, long, default_value = "hello")]
    username: String,
    #[clap(short, long, default_value = "world")]
    password: String,
}

fn try_run(args: Args) -> Result<()> {
    match args.mode {
        Mode::Server(config) => {
            let connector = tcp::ListenConnector::new(&config.listen)?;
            let connector = tls::ServerConnector {
                connector,
                pkcs12_path: config.pkcs12_path,
                pkcs12_password: config.pkcs12_password,
            };
            let connector = websocket::ListenConnector::new(
                connector,
                websocket::Authentication {
                    username: config.username,
                    password: config.password,
                },
            );
            Endpoint::new(&args.ip, &args.peer_ip, connector)?.run_with_retry();
            unreachable!();
        }
        Mode::Client(config) => {
            let connector = tcp::StreamConnector {
                addr: &config.server,
            };
            let connector = tls::ClientConnector {
                connector,
                hostname: config.hostname.clone(),
                accept_invalid_certs: !config.not_accept_invalid_certs,
            };
            let connector = websocket::ClientConnector::new(
                connector,
                format!("wss://{:}/ws", config.hostname),
                websocket::Authentication {
                    username: config.username,
                    password: config.password,
                },
            );
            let mut endpoint = Endpoint::new(&args.ip, &args.peer_ip, connector)?;

            if let Some(script_path) = config.script_path {
                let mut server_addrs_iter = config.server.to_socket_addrs().unwrap();
                let server_addr = server_addrs_iter
                    .next()
                    .ok_or(anyhow!("could not resolve server address"))?;
                let server_ip = server_addr.ip();
                run_script(
                    &script_path,
                    &format!("{:}", server_ip),
                    &args.peer_ip,
                    endpoint.tun_name(),
                    "up",
                )?;
            }

            endpoint.run_with_retry();
            unreachable!();
        }
    }
}

fn run_script(
    script_path: &str,
    server_ip: &str,
    peer_ip: &str,
    dev: &str,
    script_type: &str,
) -> Result<()> {
    let status = Command::new(script_path)
        .env("server_ip", server_ip)
        .env("peer_ip", peer_ip)
        .env("dev", dev)
        .env("script_type", script_type)
        .status()?;
    if !status.success() {
        return Err(anyhow!("script {:} failed with: {:}", script_path, status));
    }
    return Ok(());
}
