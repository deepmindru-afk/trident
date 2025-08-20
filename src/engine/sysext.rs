use std::{fs, path::Path, process::Command};

use anyhow::{Context, Error};
use log::debug;
use serde::{Deserialize, Serialize};

use osutils::exe::RunAndCheck;
use trident_api::config::HostConfiguration;

#[derive(Serialize, Deserialize, Debug, Default, Clone, PartialEq)]
struct Extension {
    name: String,
    #[serde(rename = "type")]
    ext_type: String,
    path: String,
    time: u64,
}

pub fn install_sysexts(host_config: &HostConfiguration) -> Result<(), Error> {
    // Sysexts may be stored in /etc/extensions, /run/extensions, and /var/lib/extensions All
    // sysexts are required to have extension-release.<name> file. All merged sysexts will have
    // contents in /usr/lib/extension-release.d/

    // Discover existing sysexts
    let output_json = Command::new("systemd-sysext")
        .arg("list")
        .arg("--json=pretty")
        .output_and_check()
        .context("Failed to run `systemd-sysext list --json=pretty`")?;
    let parsed: Vec<Extension> =
        serde_json::from_str(&output_json).context("Failed to parse systemd-sysext list output")?;
    debug!("Found existing extensions: {:?}", parsed);

    // Merge new sysexts
    let sysexts = &host_config.sysexts;
    for sysext in sysexts {
        // Check if systext is DDI or directory
        let sysext_name = &sysext.name;
        debug!("Sysext name is: {}", sysext_name);

        // Place sysext in /var/lib/extensions
        let sysext_new_path = Path::new("/var/lib/extensions").join(format!("{sysext_name}.raw"));
        debug!("New sysext path is: {}", sysext_new_path.display());
        fs::create_dir_all("/var/lib/extensions").context("Failed to create dirs")?;
        fs::rename(
            sysext.url.to_file_path().unwrap_or_default(),
            &sysext_new_path,
        )
        .context(format!(
            "Failed to rename from {:?} to {}",
            sysext.url.to_file_path().unwrap_or_default(),
            sysext_new_path.display()
        ))?;
        debug!(
            "Check that path '{}' exists: {}",
            sysext_new_path.display(),
            Path::exists(&sysext_new_path)
        );

        // Call systemd-sysext
        Command::new("systemd-sysext")
            .arg("merge")
            .run_and_check()
            .context("Failed to run `systemd-sysext merge`")?;
    }
    Ok(())
}
