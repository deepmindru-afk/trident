use log::{debug, info, warn};
use std::{fs, process::Command};

#[derive(Debug, PartialEq, Clone)]
pub enum BootType {
    /// System is running from a RAM disk
    RamDisk,
    /// System is running directly from CD-ROM/DVD
    LiveCdrom,
    /// System is running from persistent storage
    PersistentStorage,
}

pub fn detect_boot_type() -> Result<BootType, std::io::Error> {
    let cmdline = fs::read_to_string("/proc/cmdline")?;

    if cmdline.contains("root=/dev/ram0") || !cmdline.contains("root=") {
        debug!("RAM disk boot detected");
        Ok(BootType::RamDisk)
    } else if cmdline.contains("root=live:LABEL=CDROM") || cmdline.contains("root=live:") {
        debug!("Live CD-ROM boot detected");
        Ok(BootType::LiveCdrom)
    } else {
        debug!("Persistent storage boot detected");
        Ok(BootType::PersistentStorage)
    }
}

pub fn eject_media() -> Result<(), std::io::Error> {
    info!("Attempting to eject installation media");

    match Command::new("eject").args(["--cdrom", "--force"]).output() {
        Ok(output) if output.status.success() => {
            info!("Successfully ejected installation media");
            Ok(())
        }
        Ok(output) => {
            warn!("eject command failed with exit code: {}", output.status);
            if !output.stderr.is_empty() {
                warn!("stderr: {}", String::from_utf8_lossy(&output.stderr));
            }
            Err(std::io::Error::other(format!(
                "eject command failed with exit code: {}",
                output.status
            )))
        }
        Err(e) => {
            warn!("Failed to execute eject command: {e:?}");
            Err(e)
        }
    }
}

pub fn media_ejection() {
    match detect_boot_type() {
        Ok(BootType::RamDisk) => {
            if let Err(e) = eject_media() {
                warn!("Failed to eject installation media: {e:?}");
            }
        }
        Ok(BootType::LiveCdrom) => {
            info!("Please remove the installation media when the system reboots.");
        }
        Ok(BootType::PersistentStorage) => {
            debug!("No installation media ejection needed");
        }
        Err(e) => {
            warn!("Unable to detect boot type: {e:?} - skipping installation media ejection");
        }
    }
}
