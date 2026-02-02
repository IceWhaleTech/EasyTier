use anyhow::{bail,Context, Result};
use std::process::Command;

pub fn get_default_gateway() -> Result<String> {
    let output = Command::new("ip")
        .args(["route", "show", "default"])
        .output()?;

    if !output.status.success() {
        bail!("ip route command failed");
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    stdout
        .lines()
        .map(|l| l.trim())
        .find_map(|line| {
            line.strip_prefix("default via")
                .and_then(|_| line.split_whitespace().nth(2))
                .map(|v| v.to_string())
        })
        .context("can't find default gateway")
}
