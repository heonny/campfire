//! Port-conflict helpers: a quick "is this port taken?" probe and detection of
//! ports assigned to more than one server in the config.

use crate::model::ServerConfig;
use std::collections::{BTreeSet, HashMap};
use std::net::TcpListener;

/// Whether `port` can be bound on localhost right now. A successful bind (which
/// is immediately released) means free; `AddrInUse` means something is already
/// listening. This is a point-in-time check, not a reservation.
pub fn is_port_free(port: u16) -> bool {
    TcpListener::bind(("127.0.0.1", port)).is_ok()
}

/// Ports assigned to more than one server in `servers` — a config-level clash
/// the user should resolve before running both.
pub fn duplicate_config_ports(servers: &[ServerConfig]) -> BTreeSet<u16> {
    let mut counts: HashMap<u16, usize> = HashMap::new();
    for server in servers {
        if let Some(port) = server.port {
            *counts.entry(port).or_insert(0) += 1;
        }
    }
    counts
        .into_iter()
        .filter(|(_, count)| *count > 1)
        .map(|(port, _)| port)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Preset;

    #[test]
    fn bound_port_is_not_free_and_frees_after_drop() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        assert!(!is_port_free(port), "listener still holds the port");
        drop(listener);
        assert!(is_port_free(port), "port should be free after release");
    }

    fn server_on_port(name: &str, port: Option<u16>) -> ServerConfig {
        let mut server = ServerConfig::from_preset(name, "/tmp", Preset::Custom);
        server.port = port;
        server
    }

    #[test]
    fn finds_only_duplicated_ports() {
        let servers = vec![
            server_on_port("a", Some(8080)),
            server_on_port("b", Some(8080)), // duplicate of a
            server_on_port("c", Some(3000)),
            server_on_port("d", None),
            server_on_port("e", None), // two None ports are not a conflict
        ];
        let dups = duplicate_config_ports(&servers);
        assert!(dups.contains(&8080));
        assert!(!dups.contains(&3000));
        assert_eq!(dups.len(), 1);
    }
}
