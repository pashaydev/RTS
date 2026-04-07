#[cfg(not(target_arch = "wasm32"))]
use std::net::UdpSocket;

#[cfg(not(target_arch = "wasm32"))]
pub fn detect_lan_ip() -> Option<String> {
    let socket = UdpSocket::bind("0.0.0.0:0").ok()?;
    socket.connect("8.8.8.8:80").ok()?;
    let addr = socket.local_addr().ok()?;
    Some(addr.ip().to_string())
}

#[cfg(not(target_arch = "wasm32"))]
#[derive(Debug, Clone)]
pub struct DetectedIp {
    pub ip: String,
    pub name: String,
    pub is_likely_vpn: bool,
}

#[cfg(not(target_arch = "wasm32"))]
pub fn detect_all_ips() -> Vec<DetectedIp> {
    let mut results = Vec::new();
    if let Ok(ifaces) = if_addrs::get_if_addrs() {
        for iface in ifaces {
            if iface.is_loopback() {
                continue;
            }
            let addr = iface.addr.ip();
            if !addr.is_ipv4() {
                continue;
            }
            let ip = addr.to_string();
            let name_lower = iface.name.to_lowercase();
            let is_likely_vpn = name_lower.contains("ham")
                || name_lower.contains("tun")
                || name_lower.contains("tap")
                || name_lower.starts_with("zt")
                || name_lower.starts_with("wg")
                || name_lower.contains("vpn")
                || ip.starts_with("25.")
                || ip.starts_with("5.");
            results.push(DetectedIp {
                ip,
                name: iface.name.clone(),
                is_likely_vpn,
            });
        }
    }
    results.sort_by(|a, b| {
        b.is_likely_vpn
            .cmp(&a.is_likely_vpn)
            .then(a.name.cmp(&b.name))
    });
    results
}
