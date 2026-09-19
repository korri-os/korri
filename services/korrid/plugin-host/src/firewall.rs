//! The host owns filter/korri-plugins in both IP families. Ports use the
//! existing NixOS firewall option names; native programs never receive a
//! firewall command contribution. The host lock serializes reconciliation.
use crate::{package, process};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
    time::Duration,
};

const CHAIN: &str = "korri-plugins";
const TIMEOUT: Duration = Duration::from_secs(15);

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Ports {
    #[serde(default, rename = "allowedTCPPorts")]
    pub allowed_tcp_ports: Vec<u16>,
    #[serde(default, rename = "allowedUDPPorts")]
    pub allowed_udp_ports: Vec<u16>,
}

impl Ports {
    pub fn validate(&self) -> Result<(), String> {
        for ports in [&self.allowed_tcp_ports, &self.allowed_udp_ports] {
            if ports.len() > 128
                || ports.contains(&0)
                || ports.iter().collect::<BTreeSet<_>>().len() != ports.len()
            {
                return Err(
                    "ports must contain at most 128 distinct ports from 1 to 65535 per protocol"
                        .into(),
                );
            }
        }
        Ok(())
    }
    pub fn is_empty(&self) -> bool {
        self.allowed_tcp_ports.is_empty() && self.allowed_udp_ports.is_empty()
    }
}

pub struct Firewall {
    pub ipv4: PathBuf,
    pub ipv6: PathBuf,
}

impl Firewall {
    pub fn apply(&self, id: &str, ports: &Ports) -> Result<(), String> {
        ports.validate()?;
        if ports.is_empty() {
            return self.remove(id);
        }
        let result = (|| {
            for tool in [&self.ipv4, &self.ipv6] {
                self.prepare(tool)?;
                self.remove_from(tool, id)?;
                for (protocol, ports) in [
                    ("tcp", &ports.allowed_tcp_ports),
                    ("udp", &ports.allowed_udp_ports),
                ] {
                    for port in ports {
                        checked(
                            tool,
                            &[
                                "-A",
                                CHAIN,
                                "-p",
                                protocol,
                                "--dport",
                                &port.to_string(),
                                "-m",
                                "comment",
                                "--comment",
                                &package::unit_name(id),
                                "-j",
                                "ACCEPT",
                            ],
                        )?;
                    }
                }
            }
            Ok(())
        })();
        if let Err(error) = result {
            return match self.remove(id) {
                Ok(()) => Err(error),
                Err(cleanup) => Err(format!("{error}; firewall cleanup failed: {cleanup}")),
            };
        }
        Ok(())
    }

    pub fn remove(&self, id: &str) -> Result<(), String> {
        let mut errors = Vec::new();
        for tool in [&self.ipv4, &self.ipv6] {
            if let Err(error) = self.remove_from(tool, id) {
                if !is_unsupported(&error) {
                    errors.push(error);
                }
            }
        }
        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors.join("; "))
        }
    }

    fn prepare(&self, tool: &Path) -> Result<(), String> {
        // Query the filter table first: unavailable IPv6/backend is an error,
        // not an excuse to leave only one family protected.
        checked(tool, &["-S", "INPUT"])?;
        if !run(tool, &["-S", CHAIN])?.success {
            checked(tool, &["-N", CHAIN])?;
        }
        // Our jump must be first even if a firewall reload moved it. Delete
        // every old copy so repeated restore never accumulates INPUT rules.
        while run(tool, &["-C", "INPUT", "-j", CHAIN])?.success {
            checked(tool, &["-D", "INPUT", "-j", CHAIN])?;
        }
        checked(tool, &["-I", "INPUT", "1", "-j", CHAIN])?;
        Ok(())
    }

    fn remove_from(&self, tool: &Path, id: &str) -> Result<(), String> {
        let rules = match checked(tool, &["-S"]) {
            Ok(rules) => rules,
            Err(error) if is_unsupported(&error) => return Ok(()),
            Err(error) => return Err(error),
        };
        let marker = package::unit_name(id);
        let indices: Vec<_> = rules
            .lines()
            .filter(|line| line.starts_with(&format!("-A {CHAIN} ")))
            .enumerate()
            .filter_map(|(index, line)| {
                let words: Vec<_> = line.split_whitespace().collect();
                words
                    .windows(2)
                    .any(|pair| pair[0] == "--comment" && pair[1].trim_matches('"') == marker)
                    .then_some(index + 1)
            })
            .collect();
        for index in indices.into_iter().rev() {
            checked(tool, &["-D", CHAIN, &index.to_string()])?;
        }
        Ok(())
    }
}

fn run(tool: &Path, args: &[&str]) -> Result<process::Output, String> {
    process::run(
        tool,
        ["-w", "10"].into_iter().chain(args.iter().copied()),
        TIMEOUT,
    )
}
fn checked(tool: &Path, args: &[&str]) -> Result<String, String> {
    process::checked(
        tool,
        ["-w", "10"].into_iter().chain(args.iter().copied()),
        TIMEOUT,
    )
}

fn is_unsupported(error: &str) -> bool {
    error.contains("Protocol not supported")
        || error.contains("Failed to initialize nft")
        || error.contains("Table does not exist")
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ports_use_nixos_protocol_lists_without_ranges_or_permission_dsl() {
        let ports: Ports =
            serde_json::from_str(r#"{"allowedUDPPorts":[41641],"allowedTCPPorts":[443]}"#).unwrap();
        ports.validate().unwrap();
        for value in [
            r#"{"allowedUDPPorts":[0]}"#,
            r#"{"allowedTCPPorts":[443,443]}"#,
        ] {
            assert!(serde_json::from_str::<Ports>(value)
                .unwrap()
                .validate()
                .is_err());
        }
        for value in [
            r#"{"udp":[41641]}"#,
            r#"{"allowedUDPPorts":[65536]}"#,
            r#"{"allowedTCPPorts":null}"#,
        ] {
            assert!(serde_json::from_str::<Ports>(value).is_err());
        }
    }

    #[test]
    fn unsupported_netfilter_errors_are_detected() {
        assert!(is_unsupported(
            "iptables: Failed to initialize nft: Protocol not supported"
        ));
        assert!(is_unsupported(
            "ip6tables: Failed to initialize nft: Protocol not supported"
        ));
        assert!(is_unsupported(
            "iptables v1.8.11: can't initialize iptables 'filter': Table does not exist"
        ));
        assert!(!is_unsupported(
            "iptables: Resource temporarily unavailable"
        ));
        assert!(!is_unsupported(
            "iptables: Bad rule (does a matching rule exist in that chain?)"
        ));
    }
}
