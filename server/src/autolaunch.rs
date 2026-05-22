pub fn set_auto_launch(enabled: bool) -> Result<(), String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    platform::set_auto_launch(enabled, &exe)
}

#[cfg(target_os = "windows")]
mod platform {
    use std::io::ErrorKind;
    use std::path::Path;
    use winreg::RegKey;
    use winreg::enums::*;

    const APP_NAME: &str = "Sofamote";
    const RUN_KEY: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\Run";
    const LEGACY_RUN_VALUE_NAMES: &[&str] = &["RemoteMediaControl", "Remote Media Control"];
    const LEGACY_WRAPPER_DIR_NAMES: &[&str] = &["sofamote", "remote-media-control"];

    pub fn set_auto_launch(enabled: bool, exe: &Path) -> Result<(), String> {
        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        let run = hkcu
            .open_subkey_with_flags(RUN_KEY, KEY_SET_VALUE | KEY_QUERY_VALUE)
            .map_err(|e| e.to_string())?;

        cleanup_legacy_auto_launch(&run).map_err(|e| e.to_string())?;

        if enabled {
            let cmd = format!("\"{}\"", exe.display());
            run.set_value(APP_NAME, &cmd).map_err(|e| e.to_string())
        } else {
            run.delete_value(APP_NAME).or(Ok(()))
        }
    }

    fn cleanup_legacy_auto_launch(run: &RegKey) -> std::io::Result<()> {
        remove_legacy_run_values(run)?;
        remove_legacy_wrappers()
    }

    fn remove_legacy_run_values(run: &RegKey) -> std::io::Result<()> {
        for value_name in LEGACY_RUN_VALUE_NAMES {
            match run.delete_value(value_name) {
                Ok(()) => {}
                Err(e) if e.kind() == ErrorKind::NotFound => {}
                Err(e) => return Err(e),
            }
        }

        Ok(())
    }

    fn remove_legacy_wrappers() -> std::io::Result<()> {
        let Some(config_dir) = dirs::config_dir() else {
            return Ok(());
        };

        for dir_name in LEGACY_WRAPPER_DIR_NAMES {
            let vbs = config_dir.join(dir_name).join("start.vbs");
            match std::fs::remove_file(vbs) {
                Ok(()) => {}
                Err(e) if e.kind() == ErrorKind::NotFound => {}
                Err(e) => return Err(e),
            }
        }

        Ok(())
    }
}

#[cfg(target_os = "linux")]
mod platform {
    use std::path::Path;

    const DESKTOP_FILE: &str = "sofamote.desktop";

    pub fn set_auto_launch(enabled: bool, exe: &Path) -> Result<(), String> {
        let autostart = dirs::config_dir().ok_or("no config dir")?.join("autostart");
        let desktop = autostart.join(DESKTOP_FILE);
        if enabled {
            std::fs::create_dir_all(&autostart).map_err(|e| e.to_string())?;
            let content = format!(
                "[Desktop Entry]\nType=Application\nName=Sofamote\nExec={}\nHidden=false\nNoDisplay=false\nX-GNOME-Autostart-enabled=true\n",
                exe.display()
            );
            std::fs::write(&desktop, content).map_err(|e| e.to_string())
        } else {
            std::fs::remove_file(&desktop).or(Ok(()))
        }
    }
}

#[cfg(target_os = "macos")]
mod platform {
    use std::path::Path;

    const PLIST_FILE: &str = "com.sofamote.plist";

    pub fn set_auto_launch(enabled: bool, exe: &Path) -> Result<(), String> {
        let agents_dir = dirs::home_dir()
            .ok_or("no home dir")?
            .join("Library")
            .join("LaunchAgents");
        let plist = agents_dir.join(PLIST_FILE);
        if enabled {
            std::fs::create_dir_all(&agents_dir).map_err(|e| e.to_string())?;
            let content = format!(
                "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
                 <!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n\
                 <plist version=\"1.0\">\n\
                 <dict>\n\
                   <key>Label</key><string>com.sofamote</string>\n\
                   <key>ProgramArguments</key>\n\
                   <array><string>{}</string></array>\n\
                   <key>RunAtLoad</key><true/>\n\
                 </dict>\n\
                 </plist>\n",
                exe.display()
            );
            std::fs::write(&plist, content).map_err(|e| e.to_string())
        } else {
            std::fs::remove_file(&plist).or(Ok(()))
        }
    }
}

#[cfg(not(any(target_os = "windows", target_os = "linux", target_os = "macos")))]
mod platform {
    use std::path::Path;
    pub fn set_auto_launch(_enabled: bool, _exe: &Path) -> Result<(), String> {
        Err("auto-launch not supported on this platform".into())
    }
}
