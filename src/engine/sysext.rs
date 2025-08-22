use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    str::FromStr,
};

use anyhow::{Context, Error};
use etc_os_release::OsRelease;
use log::debug;
use serde::{Deserialize, Serialize};

use osutils::{dependencies::Dependency, exe::RunAndCheck};
use trident_api::config::{HostConfiguration, Sysext};

#[derive(Serialize, Deserialize, Debug, Default, Clone, PartialEq)]
struct Extension {
    name: String,
    #[serde(rename = "type")]
    ext_type: String,
    path: String,
    time: u64,
}

pub fn get_extension_release(img_path: &PathBuf, name: &String) -> Result<OsRelease, Error> {
    let mount_point = "/mnt/tmp";
    let release_path = Path::new(mount_point).join(format!(
        "usr/lib/extension-release.d/extension-release.{name}"
    ));
    Dependency::Losetup
        .cmd()
        .arg("-f")
        .arg("--show")
        .arg(img_path)
        .run_and_check()
        .with_context(|| "Failed to setup loop device")?;
    Dependency::Mount
        .cmd()
        .arg(img_path)
        .arg(mount_point)
        .run_and_check()
        .with_context(|| "Failed to mount sysext")?;

    // Get extension release file
    let extension_release_file_content = fs::read_to_string(release_path)
        .with_context(|| "Failed to read extension-release file content")?;
    debug!("Found extension release file content:\n {extension_release_file_content}");
    OsRelease::from_str(&extension_release_file_content)
        .with_context(|| "Failed to convert extension release file content to OsRelease object")
}

fn find_existing_sysext(
    name: &String,
    existing: &Vec<Extension>,
) -> Result<Option<OsRelease>, Error> {
    // All sysexts are required to have extension-release.<name> file. All merged sysexts will have
    // contents in /usr/lib/extension-release.d/
    for ext in existing {
        if ext.name == *name {
            let existing_extension_release_content = fs::read_to_string(format!(
                "usr/lib/extension-release.d/extension-release.{}",
                ext.name
            ))
            .with_context(|| {
                format!(
                    "Failed to read file from 'usr/lib/extension-release.d/extension-release.{}'",
                    ext.name
                )
            })?;
            let existing_ext_release = OsRelease::from_str(&existing_extension_release_content)
                .context("Failed to convert string to OsRelease object")?;
            return Ok(Some(existing_ext_release));
        }
    }
    Ok(None)
}

fn get_list_of_sysexts_to_merge(
    new: &Vec<Sysext>,
    existing: &Vec<Extension>,
) -> Result<Vec<Sysext>, Error> {
    let mut to_merge = Vec::new();
    for sysext in new {
        let sysext_name = &sysext.name;
        debug!("Sysext name is: {}", sysext_name);

        if let Ok(Some(existing_ext_release)) = find_existing_sysext(&sysext.name, existing) {
            // Get sysext version
            let current_file_path = sysext.url.to_file_path().unwrap_or_default();
            let new_extension_release = get_extension_release(&current_file_path, sysext_name)
                .with_context(|| "Failed to get extension release file")?;
            let sysext_version = OsRelease::get_value(&new_extension_release, "SYSEXT_VERSION_ID")
                .context("Failed to retrieve key 'SYSEXT_VERSION_ID'")?;
            debug!("Found sysext version ID: {sysext_version}");

            let existing_sysext_version = OsRelease::get_value(
            &existing_ext_release,
            "SYSEXT_VERSION_ID",
        )
        .context(
            "Failed to retrieve key 'SYSEXT_VERSION_ID' from existing sysext's ext release file",
        )?;
            if existing_sysext_version != sysext_version {
                to_merge.push(sysext.clone());
            }
        }
        // If there are no existing sysexts that match this new one, merge the new one
        to_merge.push(sysext.clone());
    }
    Ok(to_merge)
}

pub fn install_sysexts(host_config: &HostConfiguration) -> Result<(), Error> {
    // Discover existing sysexts
    let output_json = Command::new("systemd-sysext")
        .arg("list")
        .arg("--json=pretty")
        .output_and_check()
        .context("Failed to run `systemd-sysext list --json=pretty`")?;
    let parsed: Vec<Extension> =
        serde_json::from_str(&output_json).context("Failed to parse systemd-sysext list output")?;
    debug!("Found existing extensions: {:?}", parsed);

    let sysexts_to_merge = get_list_of_sysexts_to_merge(&host_config.sysexts, &parsed)
        .with_context(|| "Failed to get list of sysexts to merge")?;
    debug!("Merging the following extensions: {:?}", sysexts_to_merge);

    // Merge new sysexts
    for sysext in sysexts_to_merge {
        let sysext_name = &sysext.name;
        debug!("Preparing to merge: {}", sysext_name);

        let current_file_path = sysext.url.to_file_path().unwrap_or_default();

        // Place sysext in /var/lib/extensions. Sysexts may be stored in /etc/extensions,
        // /run/extensions, and /var/lib/extensions.
        let sysext_new_path = Path::new("/var/lib/extensions").join(format!("{sysext_name}.raw"));
        debug!("New sysext path is: {}", sysext_new_path.display());
        fs::create_dir_all("/var/lib/extensions").context("Failed to create dirs")?;
        fs::copy(&current_file_path, &sysext_new_path).context(format!(
            "Failed to rename from {:?} to {}",
            current_file_path,
            sysext_new_path.display()
        ))?;
        debug!(
            "Check that path '{}' exists: {}",
            sysext_new_path.display(),
            Path::exists(&sysext_new_path)
        );
    }
    // Call systemd-sysext
    Command::new("systemd-sysext")
        .arg("refresh")
        .run_and_check()
        .context("Failed to run `systemd-sysext refresh`")?;
    Ok(())
}
