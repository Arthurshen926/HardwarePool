//! Fixed helper boundary. Preserve private server/config routing, not upstream
//! stream overrides that are applied after stream rules and explicit properties.
use std::process::Command;
pub(crate) const STREAM_OVERRIDES: [&str; 6] = [
    "PIPEWIRE_NODE",
    "PIPEWIRE_AUTOCONNECT",
    "PIPEWIRE_PROPS",
    "PIPEWIRE_QUANTUM",
    "PIPEWIRE_LATENCY",
    "PIPEWIRE_RATE",
];
pub(crate) fn fixed_command() -> Command {
    let mut command = Command::new("/usr/bin/pw-cat");
    for key in STREAM_OVERRIDES {
        command.env_remove(key);
    }
    command
}
#[cfg(test)]
mod tests {
    #[test]
    fn all_stream_overrides_removed_private_routing_unchanged() {
        let command = super::fixed_command();
        assert_eq!(command.get_program(), "/usr/bin/pw-cat");
        let vars: Vec<_> = command.get_envs().collect();
        assert_eq!(vars.len(), super::STREAM_OVERRIDES.len());
        for key in super::STREAM_OVERRIDES {
            assert!(vars.iter().any(|(k, v)| *k == key && v.is_none()));
        }
    }
}
