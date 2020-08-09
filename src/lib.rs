mod endpoint;
pub mod retry;
pub mod tcp;
pub mod websocket;

pub use endpoint::*;

/*
 * let output = std::process::Command::new("ip")
 *     .args(&["address"])
 *     .output()
 *     .expect("failed to execute process");
 */
