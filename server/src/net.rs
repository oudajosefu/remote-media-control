use std::net::IpAddr;

pub fn list_lan_ips() -> Vec<IpAddr> {
    local_ip_address::list_afinet_netifas()
        .map(|ifaces| {
            ifaces
                .into_iter()
                .map(|(_, ip)| ip)
                .filter(|ip| ip.is_ipv4() && !ip.is_loopback() && !ip.is_unspecified())
                .collect()
        })
        .unwrap_or_default()
}

pub fn pick_lan_ip(previous: Option<IpAddr>) -> String {
    let candidates = list_lan_ips();

    if let Some(prev) = previous
        && candidates.contains(&prev)
    {
        return prev.to_string();
    }

    if let Ok(default) = local_ip_address::local_ip()
        && candidates.contains(&default)
    {
        return default.to_string();
    }

    candidates
        .into_iter()
        .next()
        .map(|ip| ip.to_string())
        .unwrap_or_else(|| "127.0.0.1".to_string())
}

pub fn get_lan_ip() -> String {
    pick_lan_ip(None)
}
