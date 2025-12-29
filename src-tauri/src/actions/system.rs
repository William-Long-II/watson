use std::process::Command;

#[derive(Debug, Clone)]
pub struct SystemCommand {
    pub id: String,
    pub name: String,
    pub aliases: Vec<String>,
    pub description: String,
    pub requires_confirmation: bool,
}

pub fn get_system_commands() -> Vec<SystemCommand> {
    vec![
        SystemCommand {
            id: "cmd:lock".to_string(),
            name: "Lock".to_string(),
            aliases: vec!["lock".to_string(), "lockscreen".to_string()],
            description: "Lock the screen".to_string(),
            requires_confirmation: false,
        },
        SystemCommand {
            id: "cmd:sleep".to_string(),
            name: "Sleep".to_string(),
            aliases: vec!["sleep".to_string()],
            description: "Put computer to sleep".to_string(),
            requires_confirmation: false,
        },
        SystemCommand {
            id: "cmd:restart".to_string(),
            name: "Restart".to_string(),
            aliases: vec!["restart".to_string(), "reboot".to_string()],
            description: "Restart the computer".to_string(),
            requires_confirmation: true,
        },
        SystemCommand {
            id: "cmd:shutdown".to_string(),
            name: "Shutdown".to_string(),
            aliases: vec!["shutdown".to_string(), "poweroff".to_string()],
            description: "Shut down the computer".to_string(),
            requires_confirmation: true,
        },
        SystemCommand {
            id: "cmd:logout".to_string(),
            name: "Log Out".to_string(),
            aliases: vec!["logout".to_string(), "signout".to_string()],
            description: "Log out current user".to_string(),
            requires_confirmation: true,
        },
        SystemCommand {
            id: "cmd:emptytrash".to_string(),
            name: "Empty Trash".to_string(),
            aliases: vec!["emptytrash".to_string(), "trash".to_string()],
            description: "Empty the trash/recycle bin".to_string(),
            requires_confirmation: true,
        },
        SystemCommand {
            id: "cmd:mute".to_string(),
            name: "Mute".to_string(),
            aliases: vec!["mute".to_string()],
            description: "Mute system audio".to_string(),
            requires_confirmation: false,
        },
        SystemCommand {
            id: "cmd:unmute".to_string(),
            name: "Unmute".to_string(),
            aliases: vec!["unmute".to_string()],
            description: "Unmute system audio".to_string(),
            requires_confirmation: false,
        },
    ]
}

#[cfg(target_os = "macos")]
pub fn execute_command(command_id: &str) -> Result<(), String> {
    match command_id {
        "cmd:lock" => {
            Command::new("pmset")
                .args(["displaysleepnow"])
                .spawn()
                .map_err(|e| e.to_string())?;
        }
        "cmd:sleep" => {
            Command::new("pmset")
                .args(["sleepnow"])
                .spawn()
                .map_err(|e| e.to_string())?;
        }
        "cmd:restart" => {
            Command::new("osascript")
                .args(["-e", "tell app \"System Events\" to restart"])
                .spawn()
                .map_err(|e| e.to_string())?;
        }
        "cmd:shutdown" => {
            Command::new("osascript")
                .args(["-e", "tell app \"System Events\" to shut down"])
                .spawn()
                .map_err(|e| e.to_string())?;
        }
        "cmd:logout" => {
            Command::new("osascript")
                .args(["-e", "tell app \"System Events\" to log out"])
                .spawn()
                .map_err(|e| e.to_string())?;
        }
        "cmd:emptytrash" => {
            Command::new("osascript")
                .args(["-e", "tell app \"Finder\" to empty the trash"])
                .spawn()
                .map_err(|e| e.to_string())?;
        }
        "cmd:mute" => {
            Command::new("osascript")
                .args(["-e", "set volume with output muted"])
                .spawn()
                .map_err(|e| e.to_string())?;
        }
        "cmd:unmute" => {
            Command::new("osascript")
                .args(["-e", "set volume without output muted"])
                .spawn()
                .map_err(|e| e.to_string())?;
        }
        _ => return Err(format!("Unknown command: {}", command_id)),
    }
    Ok(())
}

#[cfg(target_os = "windows")]
pub fn execute_command(command_id: &str) -> Result<(), String> {
    match command_id {
        "cmd:lock" => {
            Command::new("rundll32.exe")
                .args(["user32.dll,LockWorkStation"])
                .spawn()
                .map_err(|e| e.to_string())?;
        }
        "cmd:sleep" => {
            Command::new("rundll32.exe")
                .args(["powrprof.dll,SetSuspendState", "0,1,0"])
                .spawn()
                .map_err(|e| e.to_string())?;
        }
        "cmd:restart" => {
            Command::new("shutdown")
                .args(["/r", "/t", "0"])
                .spawn()
                .map_err(|e| e.to_string())?;
        }
        "cmd:shutdown" => {
            Command::new("shutdown")
                .args(["/s", "/t", "0"])
                .spawn()
                .map_err(|e| e.to_string())?;
        }
        "cmd:logout" => {
            Command::new("shutdown")
                .args(["/l"])
                .spawn()
                .map_err(|e| e.to_string())?;
        }
        "cmd:emptytrash" => {
            Command::new("powershell")
                .args(["-Command", "Clear-RecycleBin -Force"])
                .spawn()
                .map_err(|e| e.to_string())?;
        }
        "cmd:mute" => {
            Command::new("powershell")
                .args(["-Command", "(New-Object -ComObject WScript.Shell).SendKeys([char]173)"])
                .spawn()
                .map_err(|e| e.to_string())?;
        }
        "cmd:unmute" => {
            Command::new("powershell")
                .args(["-Command", "(New-Object -ComObject WScript.Shell).SendKeys([char]173)"])
                .spawn()
                .map_err(|e| e.to_string())?;
        }
        _ => return Err(format!("Unknown command: {}", command_id)),
    }
    Ok(())
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
pub fn execute_command(command_id: &str) -> Result<(), String> {
    Err(format!("System commands not supported on this platform: {}", command_id))
}
