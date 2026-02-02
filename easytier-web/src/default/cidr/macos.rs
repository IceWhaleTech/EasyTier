use anyhow::{bail, Context, Result};
use std::process::Command;

pub fn get_default_gateway() -> Result<String> {
    let output = Command::new("route")
        .args(["-n", "get", "default"])
        .output()?;

    if !output.status.success() {
        bail!("route command failed");
    }

    let stdout = String::from_utf8_lossy(&output.stdout);

    let gateway = stdout
        .lines()
        .map(|line| line.trim())
        .find(|line| line.starts_with("gateway:"))
        .context("gateway not found")?
        .split(' ')
        .nth(1)
        .context("gateway does not ip")?
        .to_string();

    Ok(gateway)
}
