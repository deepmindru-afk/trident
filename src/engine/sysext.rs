use std::{
    fs::{self},
    io,
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
use url::Url;

#[derive(Serialize, Deserialize, Debug, Default, Clone, PartialEq)]
struct Extension {
    id: Option<String>,
    sysext_id: Option<String>,
    sysext_version_id: Option<String>,
    sysext_scope: Option<String>,
    architecture: Option<String>,
    name: String,
}

fn get_extension_release_from_new_sysext(img_path: &PathBuf) -> Result<Extension, Error> {
    let mount_point = "/mnt/tmp";
    fs::create_dir_all(mount_point)
        .context(format!("Failed to create directory at '{mount_point}'"))?;
    let release_dir = Path::new(mount_point).join("usr/lib/extension-release.d/");
    let loop_device_output = Dependency::Losetup
        .cmd()
        .arg("-f")
        .arg("--show")
        .arg(img_path)
        .output_and_check()
        .with_context(|| "Failed to setup loop device")?;
    let loop_device = loop_device_output.trim();
    debug!("Created loop device: {}", loop_device);
    Dependency::Mount
        .cmd()
        .arg("-t")
        .arg("ddi")
        .arg(loop_device)
        .arg(mount_point)
        .run_and_check()
        .with_context(|| {
            format!("Failed to mount loop device '{loop_device}' at '{mount_point}'")
        })?;
    debug!("Successfully mounted loop device '{loop_device}' at '{mount_point}'");

    // Get extension release file
    let extension_release = get_extension_release(release_dir)?;

    Dependency::Umount
        .cmd()
        .arg(mount_point)
        .run_and_check()
        .context("Failed to unmount")?;
    Dependency::Losetup
        .cmd()
        .arg("-d")
        .arg(loop_device)
        .run_and_check()
        .context("Failed to detach loop device")?;

    debug!("Returning extension_release: {extension_release:?}");

    Ok(extension_release)
}

fn get_extension_release(directory: PathBuf) -> Result<Extension, Error> {
    // Get extension release file
    debug!(
        "Attempting to read from directory '{}'",
        directory.display()
    );
    let files = fs::read_dir(&directory)?
        .map(|res| res.map(|e| e.path()))
        .collect::<Result<Vec<_>, io::Error>>()?;

    // If no name was passed to this function, we expect only one file to be present in the directory
    let path = &files[0];
    debug!("Evaluating path: '{}'", path.display());
    // Find the file whose `SYSEXT_ID` matches `name` parameter
    let extension_release_file_content = fs::read_to_string(path).context(format!(
        "Failed to read extension-release file content from '{}'",
        &path.display()
    ))?;
    debug!("Found extension release file content:\n {extension_release_file_content}");
    let extension_release_obj = OsRelease::from_str(&extension_release_file_content)
        .with_context(|| "Failed to convert extension release file content to OsRelease object")?;
    let file_name = path
        .display()
        .to_string()
        .split("extension-release.")
        .last()
        .ok_or_else(|| Error::msg("Failed to get extension-release ending"))?
        .to_string();
    Ok(Extension {
        id: extension_release_obj.get_value("ID").map(|s| s.to_string()),
        sysext_id: extension_release_obj
            .get_value("SYSEXT_ID")
            .map(|s| s.to_string()),
        sysext_version_id: extension_release_obj
            .get_value("SYSEXT_VERSION_ID")
            .map(|s| s.to_string()),
        sysext_scope: extension_release_obj
            .get_value("SYSEXT_SCOPE")
            .map(|s| s.to_string()),
        architecture: extension_release_obj
            .get_value("ARCHITECTURE")
            .map(|s| s.to_string()),
        name: file_name,
    })
}

fn find_existing_sysext(
    new: Extension,
    existing: &Vec<Extension>,
) -> Result<Option<Extension>, Error> {
    // All sysexts are required to have extension-release.<name> file. All merged sysexts will have
    // contents in /usr/lib/extension-release.d/
    for ext in existing {
        debug!(
            "Comparing existing '{:?}' with new '{:?}'",
            ext.sysext_id, new.sysext_id
        );

        if ext.sysext_id == new.sysext_id {
            debug!("Found a matching sysext on the OS");
            return Ok(Some(ext.clone()));
        }
    }
    debug!("Did not find any matching sysext on the OS");
    Ok(None)
}

fn get_list_of_sysexts_to_merge(
    new: &Vec<Sysext>,
    existing: &Vec<Extension>,
) -> Result<Vec<(String, Url)>, Error> {
    let mut to_merge = Vec::new();
    for sysext in new {
        // Get new sysext's information
        let current_file_path = sysext.url.to_file_path().unwrap_or_default();
        let new_extension = get_extension_release_from_new_sysext(&current_file_path)
            .with_context(|| "Failed to get extension release file")?;
        debug!(
            "Found sysext version ID: {:?}",
            new_extension.sysext_version_id
        );
        debug!("Sysext name is: {}", new_extension.name);

        if let Ok(Some(existing_ext_release)) =
            find_existing_sysext(new_extension.clone(), existing)
        {
            debug!(
                "Found an existing sysext on the OS with the same SYSEXT_ID: '{:?}'",
                new_extension.sysext_id
            );
            if existing_ext_release.sysext_version_id != new_extension.sysext_version_id {
                debug!("SYSEXT_VERSION_ID does not match. Merging new version.");
                to_merge.push((new_extension.name, sysext.url.clone()));
            }
        } else {
            // If there are no existing sysexts that match this new one, merge the new one
            debug!(
                "Did not find any exisiting sysexts with SYSEXT_ID: {:?}",
                new_extension.sysext_id
            );
            to_merge.push((new_extension.name, sysext.url.clone()));
        }
    }
    Ok(to_merge)
}

fn get_existing_sysexts() -> Result<Vec<Extension>, Error> {
    let mut ret = Vec::new();
    let extension_release_dir = Path::new("/usr/lib/extension-release.d/");
    if !extension_release_dir.exists() {
        return Ok(ret);
    }
    let files = fs::read_dir(extension_release_dir)?
        .map(|res| res.map(|e| e.path()))
        .collect::<Result<Vec<_>, io::Error>>()?;
    for path in &files {
        debug!("Evaluating path: '{}'", path.display());
        let extension_release_file_content = fs::read_to_string(path).context(format!(
            "Failed to read extension-release file content from '{}'",
            &path.display()
        ))?;
        let extension_release_obj = OsRelease::from_str(&extension_release_file_content)
            .with_context(|| {
                "Failed to convert extension release file content to OsRelease object"
            })?;

        let name = path
            .display()
            .to_string()
            .split("extension-release.")
            .last()
            .context("Could not find name")?
            .to_string();

        ret.push(Extension {
            id: extension_release_obj.get_value("ID").map(|s| s.to_string()),
            sysext_id: extension_release_obj
                .get_value("SYSEXT_ID")
                .map(|s| s.to_string()),
            sysext_scope: extension_release_obj
                .get_value("SYSEXT_SCOPE")
                .map(|s| s.to_string()),
            sysext_version_id: extension_release_obj
                .get_value("SYSEXT_VERSION_ID")
                .map(|s| s.to_string()),
            architecture: extension_release_obj
                .get_value("ARCHITECTURE")
                .map(|s| s.to_string()),
            name,
        });
    }
    Ok(ret)
}

pub fn install_sysexts(host_config: &HostConfiguration) -> Result<(), Error> {
    // Discover existing sysexts
    let existing = get_existing_sysexts().context("Failed to get existing sysexts on OS")?;
    debug!("Found existing extensions: {:?}", existing);

    let sysexts_to_merge = get_list_of_sysexts_to_merge(&host_config.sysexts, &existing)
        .with_context(|| "Failed to get list of sysexts to merge")?;
    debug!("Merging the following extensions: {:?}", sysexts_to_merge);

    // Merge new sysexts
    for (sysext_name, url) in sysexts_to_merge {
        debug!("Preparing to merge: {}", sysext_name);

        let current_file_path = url.to_file_path().unwrap_or_default();

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
