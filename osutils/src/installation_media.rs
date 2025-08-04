use log::{debug, info, warn};
use std::{fs, process::Command};

#[derive(Debug, PartialEq, Clone)]
pub enum BootType {
    /// System is running from a RAM disk
    RamDisk,
    /// System is running directly from CD-ROM/DVD
    LiveCdrom,
    /// System is running from persistent storage (normal installation)
    PersistentStorage,
}

pub fn detect_boot_type() -> Result<BootType, std::io::Error> {
    let cmdline = fs::read_to_string("/proc/cmdline")?;

    if cmdline.contains("root=/dev/ram0") || !cmdline.contains("root=") {
        debug!("Detected RAM disk boot");
        Ok(BootType::RamDisk)
    } else if cmdline.contains("root=live:LABEL=CDROM") || cmdline.contains("root=live:") {
        debug!("Detected live CD-ROM boot");
        Ok(BootType::LiveCdrom)
    } else {
        debug!("Detected persistent storage boot");
        Ok(BootType::PersistentStorage)
        // TODO: Check multiboot to know how to handle this case
    }
}

/// Attempts to eject installation media based on the current boot type
pub fn eject_installation_media_smart() {
    match detect_boot_type() {
        Ok(BootType::RamDisk) => {
            eject_installation_media();
        }
        Ok(BootType::LiveCdrom) => {
            warn!("Running from live CD-ROM - cannot eject while system is active");
        }
        Ok(BootType::PersistentStorage) => {
            debug!("Running from persistent storage - no installation media to eject");
        }
        Err(e) => {
            warn!("Could not determine boot type: {e:?} - skipping automatic ejection");
        }
    }
}

/// Attempts to eject installation media immediately
fn eject_installation_media() {
    info!("Attempting to eject installation media");

    match Command::new("eject").args(["--cdrom", "--force"]).output() {
        Ok(output) if output.status.success() => {
            info!("Successfully ejected installation media");
        }
        Ok(output) => {
            warn!("eject command failed with exit code: {}", output.status);
            if !output.stderr.is_empty() {
                warn!("stderr: {}", String::from_utf8_lossy(&output.stderr));
            }
            // TODO: Do not fail, send a warning message and continue
        }
        Err(e) => {
            warn!("Failed to execute eject command: {e:?}");
            // TODO: Do not fail, send a warning message and continue
        }
    }
}
