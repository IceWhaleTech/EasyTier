mod linux;
mod macos;

use anyhow::{Context, Ok};
use pnet::datalink;
use std::net::{IpAddr, Ipv4Addr};

#[cfg(target_os = "linux")]
pub use linux::get_default_gateway;

#[cfg(target_os = "macos")]
pub use macos::get_default_gateway;

pub fn get_default_cidr() -> anyhow::Result<String> {
    let gateway: Ipv4Addr = get_default_gateway()?.parse()?;

    let ip = datalink::interfaces()
        .iter()
        .flat_map(|iface| iface.ips.iter())
        .find(|ip| ip.contains(IpAddr::from(gateway)))
        .context("can't find default gateway")?
        .to_owned();

    Ok(format!("{}/{}", ip.network().to_string(), ip.prefix()))
}

#[cfg(test)]
mod tests {
    use anyhow::Ok;

    use crate::default::cidr::get_default_cidr;

    use anyhow::Result;
    #[test]
    #[cfg(target_os = "macos")]
    fn test_cidr_macos() -> Result<()> {
        assert_eq!(get_default_cidr()?, "10.0.0.0/16".to_owned(), "success");
        Ok(())
    }
    #[test]
    #[cfg(target_os = "linux")]
    fn test_cidr_linux() -> Result<()> {
        assert_eq!(
            get_default_cidr()?,
            "192.168.139.0/24".to_owned(),
            "success"
        );
        Ok(())
    }
}
