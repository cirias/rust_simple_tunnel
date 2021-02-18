use std::net;

use anyhow::{anyhow, Result};
use clap::Clap;

use tun;

use simple_tunnel::*;

fn main() {
    env_logger::init();

    let args = Args::parse();

    if let Err(e) = run(args) {
        eprintln!("{:?}", e);
    }
}

#[derive(Clap)]
#[clap(version = "0.1")]
struct Args {
    #[clap(long, default_value = "tun0")]
    tun_name: String,
    #[clap(long, default_value = "1400")]
    tun_mtu: i32,
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

fn run(args: Args) -> Result<()> {
    match args.mode {
        Mode::Client(ref config) => run_client(&args, config),
        Mode::Server(ref config) => run_server(&args, config),
    }
}

fn run_client(args: &Args, config: &ClientConfig) -> Result<()> {
    let mut tun_config: tun::Configuration = Default::default();
    tun_config.name(&args.tun_name).mtu(args.tun_mtu).up();
    let tun =
        tun::create(&tun_config).or_else(|e| Err(anyhow!("could not create tun: {:?}", e)))?;
    let tun = sockets::read_write::Socket(tun);

    let auth = sockets::websocket::BasicAuthentication {
        username: config.username.clone(),
        password: config.password.clone(),
    };

    let ws = sockets::websocket::connect_tls_tcp(
        &config.server,
        &config.hostname,
        !config.not_accept_invalid_certs,
        auth,
    )
    .or_else(|e| Err(anyhow!("could not connect to server: {:?}", e)))?;

    message::run(ws, tun).or_else(|e| Err(anyhow!("could not run loop: {:?}", e)))
}

fn run_server(args: &Args, config: &ServerConfig) -> Result<()> {
    let mut tun_config: tun::Configuration = Default::default();
    tun_config.name(&args.tun_name).mtu(args.tun_mtu).up();
    let tun =
        tun::create(&tun_config).or_else(|e| Err(anyhow!("could not create tun: {:?}", e)))?;
    let tun = sockets::read_write::Socket(tun);

    let auth = sockets::websocket::BasicAuthentication {
        username: config.username.clone(),
        password: config.password.clone(),
    };

    let tcp_listener = net::TcpListener::bind(&config.listen)
        .or_else(|e| Err(anyhow!("could not bind tcp listenr: {:?}", e)))?;
    let ws = sockets::websocket::TlsTcpListener {
        listener: tcp_listener,
        pkcs12_path: config.pkcs12_path.clone(),
        pkcs12_password: config.pkcs12_password.clone(),
        auth,
    }
    .accept()
    .or_else(|e| Err(anyhow!("could not accept client: {:?}", e)))?;

    message::run(ws, tun).or_else(|e| Err(anyhow!("could not run loop: {:?}", e)))
}
