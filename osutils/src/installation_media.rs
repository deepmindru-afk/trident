//! Utilities for managing installation media (ejection, detection, etc.)

use std::{fs, process::Command};

use log::{debug, info, warn};

/// Represents the type of boot environment
#[derive(Debug, PartialEq, Clone)]
pub enum BootType {
    /// System is running from a RAM disk (initramfs extracted to RAM)
    RamDisk,
    /// System is running directly from CD-ROM/DVD
    LiveCdrom,
    /// System is running from persistent storage (normal installation)
    PersistentStorage,
}

/// Detects the current boot type by examining /proc/cmdline
pub fn detect_boot_type() -> Result<BootType, std::io::Error> {
    let cmdline = fs::read_to_string("/proc/cmdline")?;

    if cmdline.contains("root=/dev/ram0") {
        debug!("Detected RAM disk boot (root=/dev/ram0)");
        Ok(BootType::RamDisk)
    } else if cmdline.contains("root=live:LABEL=CDROM") || cmdline.contains("root=live:") {
        debug!("Detected live CD-ROM boot");
        Ok(BootType::LiveCdrom)
    } else {
        debug!("Detected persistent storage boot");
        Ok(BootType::PersistentStorage)
    }
}

/// Attempts to eject installation media based on the current boot type
pub fn eject_installation_media_smart() {
    match detect_boot_type() {
        Ok(BootType::RamDisk) => {
            info!("Running from RAM disk - safe to eject installation media");
            eject_installation_media();
        }
        Ok(BootType::LiveCdrom) => {
            warn!("Running from live CD-ROM - cannot eject while system is active");
            log_live_cdrom_warning();
        }
        Ok(BootType::PersistentStorage) => {
            debug!("Running from persistent storage - no installation media to eject");
        }
        Err(e) => {
            warn!("Could not determine boot type: {e:?} - skipping automatic ejection");
            log_manual_removal_message();
        }
    }
}

/// Attempts to eject installation media immediately
fn eject_installation_media() {
    info!("Attempting to eject installation media");

    // Try different eject strategies in order of preference
    let eject_commands = [
        // Standard CD-ROM/DVD eject with force
        ("eject", vec!["--cdrom", "--force"]),
        // Try specific devices
        ("eject", vec!["/dev/sr0"]),
        ("eject", vec!["/dev/cdrom"]),
        // Try without force flag as fallback
        ("eject", vec!["--cdrom"]),
    ];

    let mut ejected = false;

    for (cmd, args) in &eject_commands {
        match Command::new(cmd).args(args).output() {
            Ok(output) if output.status.success() => {
                info!(
                    "Successfully ejected installation media using: {} {}",
                    cmd,
                    args.join(" ")
                );
                ejected = true;
                break;
            }
            Ok(output) => {
                debug!(
                    "Command '{}' with args {:?} failed with exit code: {}",
                    cmd, args, output.status
                );
                if !output.stderr.is_empty() {
                    debug!("stderr: {}", String::from_utf8_lossy(&output.stderr));
                }
            }
            Err(e) => {
                debug!("Failed to execute '{}': {e:?}", cmd);
            }
        }
    }

    if !ejected {
        warn!("All eject attempts failed");
        log_manual_removal_message();
    }
}

/// Logs a warning message specific to live CD-ROM scenarios
fn log_live_cdrom_warning() {
    warn!("============================================");
    warn!("Installation Complete.");
    warn!("System is currently running from CD-ROM.");
    warn!("Installation media will remain mounted");
    warn!("until the system reboots into the new OS.");
    warn!("Please ensure the target disk is bootable.");
    warn!("============================================");
}

/// Logs a generic manual removal message
fn log_manual_removal_message() {
    warn!("============================================");
    warn!("Installation Complete.");
    warn!("Please manually remove installation media");
    warn!("before rebooting the system.");
    warn!("============================================");
}

/// Check if removable devices are present (for additional context)
pub fn check_removable_devices() -> bool {
    match Command::new("lsblk").args(["-no", "NAME,RM,TYPE"]).output() {
        Ok(output) if output.status.success() => {
            let output_str = String::from_utf8_lossy(&output.stdout);
            let has_removable = output_str.lines().any(|line| {
                let parts: Vec<&str> = line.split_whitespace().collect();
                parts.len() >= 3 && parts[1] == "1" && parts[2] == "disk"
            });

            if has_removable {
                debug!("Removable devices detected in system");
            } else {
                debug!("No removable devices detected");
            }

            has_removable
        }
        Ok(_) => {
            debug!("lsblk command succeeded but returned non-zero exit code");
            false
        }
        Err(e) => {
            debug!("Failed to check for removable devices: {e:?}");
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_boot_type_detection() {
        // This test would need to mock /proc/cmdline reading
        // For now, just test that the function doesn't panic
        let _ = detect_boot_type();
    }
}
