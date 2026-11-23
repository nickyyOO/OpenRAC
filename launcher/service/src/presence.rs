//! Discord's "Playing ..." line. Discord can take seconds to answer, so one thread of its own
//! talks to it, in the order things happened; nothing else ever waits for it.

use std::sync::mpsc::{self, Sender};

use openrac_launcher_core::config::Config;
use openrac_launcher_core::discord::{self, DiscordIpc, DiscordStatus, GameLog};

enum Message {
    /// The settings changed: the client id, and whether to show anything at all.
    Configure {
        client_id: String,
        enabled: bool,
    },
    Status(DiscordStatus),
    GameLog(GameLog),
    EndPlay,
}

/// The connection to Discord, behind its own thread.
pub struct Presence {
    send: Sender<Message>,
}

impl Presence {
    /// Connects when the setting is on and shows the launcher as idle.
    pub fn start(config: &Config) -> Presence {
        let (send, receive) = mpsc::channel::<Message>();
        let mut ipc = DiscordIpc::new(client_id(config));
        let mut enabled = config.discord_rpc;
        let app_start = discord::now_sec();
        std::thread::Builder::new()
            .name("discord".into())
            .spawn(move || {
                if enabled {
                    let _ = ipc.set_status(&DiscordStatus::Idle, app_start);
                }
                for message in receive {
                    let _ = match message {
                        Message::Configure { client_id, enabled: on } => {
                            enabled = on;
                            ipc.set_client_id(client_id);
                            if on {
                                Ok(())
                            } else {
                                ipc.clear()
                            }
                        }
                        _ if !enabled => Ok(()),
                        Message::Status(status) => ipc.set_status(&status, app_start),
                        Message::GameLog(log) => ipc.on_game_log(log),
                        Message::EndPlay => ipc.end_play(app_start),
                    };
                }
            })
            .expect("a thread for Discord");
        Presence { send }
    }

    /// Follows the settings: another client id, or the presence switched off.
    pub fn configure(&self, config: &Config) {
        let _ = self.send.send(Message::Configure { client_id: client_id(config), enabled: config.discord_rpc });
    }

    pub fn set_status(&self, status: DiscordStatus) {
        let _ = self.send.send(Message::Status(status));
    }

    /// A planet or the bolts the game logged.
    pub fn game_log(&self, log: GameLog) {
        let _ = self.send.send(Message::GameLog(log));
    }

    /// The game ended: back to the launcher's own line.
    pub fn end_play(&self) {
        let _ = self.send.send(Message::EndPlay);
    }
}

fn client_id(config: &Config) -> String {
    config.discord_client_id.clone().unwrap_or_else(|| discord::DEFAULT_CLIENT_ID.to_string())
}
