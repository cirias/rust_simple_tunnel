use std::net;

use anyhow::{anyhow, Result};
use clap::Clap;
use rand;

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
    #[clap(long, default_value = "127.0.0.1:3000")]
    server: String,
    #[clap(long, default_value = "www.example.com")]
    hostname: String,
    #[clap(long, default_value = "./ca_cert.pem")]
    ca_cert_path: String,
    #[clap(long, default_value = "hello")]
    username: String,
    #[clap(long, default_value = "world")]
    password: String,
}

#[derive(Clap)]
struct ServerConfig {
    #[clap(long, default_value = "0.0.0.0:3000")]
    listen: String,
    #[clap(long, default_value = "./cert.pem")]
    cert_path: String,
    #[clap(long, default_value = "./key.pem")]
    key_path: String,
    #[clap(long, default_value = "hello")]
    username: String,
    #[clap(long, default_value = "world")]
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
    let mut tun = sockets::read_write::Socket(tun);

    let auth = sockets::websocket::BasicAuthentication {
        username: config.username.clone(),
        password: config.password.clone(),
    };

    let ws_client =
        sockets::websocket::TlsTcpConnector::new(&config.hostname, &config.ca_cert_path, auth)
            .or_else(|e| Err(anyhow!("could not create connector: {:?}", e)))?;

    let sleep_ms = 200;
    loop {
        let ws = match ws_client
            .connect(&config.server)
            .or_else(|e| Err(anyhow!("could not connect to server: {:?}", e)))
        {
            Err(e) => {
                eprintln!("{:?}, will retry", e);
                let jitter_ms = rand::random::<u16>() >> 6; // 0 - 1023
                std::thread::sleep(std::time::Duration::from_millis(
                    sleep_ms + jitter_ms as u64,
                ));
                continue;
            }
            Ok(ws) => ws,
        };

        datagram::run(ws, &mut tun)
            .or_else(|e| Err(anyhow!("could not run loop: {:?}", e)))
            .unwrap_or_else(|e| eprintln!("{:?}, will retry", e));
    }
}

fn run_server(args: &Args, config: &ServerConfig) -> Result<()> {
    let mut tun_config: tun::Configuration = Default::default();
    tun_config.name(&args.tun_name).mtu(args.tun_mtu).up();
    let tun =
        tun::create(&tun_config).or_else(|e| Err(anyhow!("could not create tun: {:?}", e)))?;
    let mut tun = sockets::read_write::Socket(tun);

    let auth = sockets::websocket::BasicAuthentication {
        username: config.username.clone(),
        password: config.password.clone(),
    };

    let tcp_listener = net::TcpListener::bind(&config.listen)
        .or_else(|e| Err(anyhow!("could not bind tcp listenr: {:?}", e)))?;
    let ws = sockets::websocket::TlsTcpListener::new(
        tcp_listener,
        &config.cert_path,
        &config.key_path,
        auth,
    )
    .or_else(|e| Err(anyhow!("could not create listener: {:?}", e)))?
    .accept()
    .or_else(|e| Err(anyhow!("could not accept client: {:?}", e)))?;

    datagram::run(ws, &mut tun).or_else(|e| Err(anyhow!("could not run loop: {:?}", e)))
}
