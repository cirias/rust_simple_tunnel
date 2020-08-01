mod endpoint;
pub mod tcp;

pub use endpoint::*;

/*
 * let output = std::process::Command::new("ip")
 *     .args(&["address"])
 *     .output()
 *     .expect("failed to execute process");
 */
