use super::cidr::get_default_cidr;

use easytier::common::config::{ConfigLoader, PeerConfig, TomlConfigLoader};

pub type NetworkConfig = easytier::proto::api::manage::NetworkConfig;
pub type NetworkIdentity = easytier::common::config::NetworkIdentity;

mod ice_whale {
    pub const TLD_DNS_ZONE: &'static str = ".ice-whale.io.";
    pub const THREAD_COUNT: usize = 4;
    pub const DHCP: bool = true;
}
#[derive(Debug, serde::Deserialize, serde::Serialize)]
enum QuickTunnel {
    QUIC,
    KCP,
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub struct IceWhaleNetworkConfig {
    #[serde(flatten)]
    indent: NetworkIdentity,
    listeners: Vec<url::Url>,
    peers: Vec<PeerConfig>,
    tld_dns_zone: Option<String>,
    quick_tunnel: QuickTunnel,
}

impl TryFrom<IceWhaleNetworkConfig> for NetworkConfig {
    type Error = anyhow::Error;
    fn try_from(config: IceWhaleNetworkConfig) -> Result<Self, Self::Error> {
        let cfg = TomlConfigLoader::default();
        let inst_id = uuid::Uuid::new_v4();
        cfg.set_id(inst_id);
        cfg.set_dhcp(ice_whale::DHCP);
        cfg.set_network_identity(config.indent);
        let mut flags = cfg.get_flags();
        flags.latency_first = true;
        flags.tld_dns_zone = config
            .tld_dns_zone
            .unwrap_or(ice_whale::TLD_DNS_ZONE.to_owned());
        flags.accept_dns = true;
        flags.multi_thread = true;

        flags.multi_thread_count = std::thread::available_parallelism()
            .unwrap_or(std::num::NonZeroUsize::new(ice_whale::THREAD_COUNT).unwrap())
            .get() as u32;
        
        match config.quick_tunnel {
            QuickTunnel::QUIC => flags.enable_quic_proxy = true,
            QuickTunnel::KCP => flags.enable_kcp_proxy = true,
        }
        cfg.set_flags(flags);
        cfg.add_proxy_cidr(get_default_cidr()?.parse()?, None)?;
        cfg.set_listeners(config.listeners);
        cfg.set_peers(config.peers);
        let network_config = NetworkConfig::new_from_config(cfg)?;
        Ok(network_config)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_default_network_config() -> anyhow::Result<()> {
        let ice_config = IceWhaleNetworkConfig {
            indent: NetworkIdentity::default(),
            listeners: vec![
                "tcp://0.0.0.0:11010".parse()?,
                "udp://0.0.0.0:11011".parse()?,
            ],
            peers: vec![
                PeerConfig {
                    uri: "tcp://1.1.1.1:11010".parse()?,
                    peer_public_key: None,
                },
                PeerConfig {
                    uri: "tcp://2.2.2.2:11010".parse()?,
                    peer_public_key: None,
                },
            ],
            tld_dns_zone: Some(".ice-whale.io.".to_string()),
            quick_tunnel: QuickTunnel::QUIC,
        };

        let config = NetworkConfig::try_from(ice_config)?;
        println!("{}", toml::to_string_pretty(&config)?);
        Ok(())
    }
}
