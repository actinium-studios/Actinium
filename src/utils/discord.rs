// discord.rs
// (c) 2026 Actinium Studios
// License:
use discord_rich_presence::{activity, DiscordIpc, DiscordIpcClient};
use std::sync::mpsc::{self, Sender};
use std::thread;
use std::time::Duration;

pub enum PresenceUpdate {
    Set { state: String, details: String },
    Clear,
}

pub struct Presence {
    tx: Sender<PresenceUpdate>,
}

impl Presence {
    pub fn start(app_id: &'static str) -> Self {
        let (tx, rx) = mpsc::channel::<PresenceUpdate>();

        thread::spawn(move || {
            let mut client: Option<DiscordIpcClient> = None;
            let mut last: Option<(String, String)> = None;

            loop {
                // (re)connect if needed
                if client.is_none() {
                    let mut c = DiscordIpcClient::new(app_id);
                    if c.connect().is_ok() {
                        // re-apply the last known state after reconnecting
                        if let Some((s, d)) = &last {
                            let _ = c.set_activity(activity::Activity::new().state(s).details(d));
                        }
                        client = Some(c);
                    }
                }

                // wait for an update, or time out and loop to retry connecting
                match rx.recv_timeout(Duration::from_secs(15)) {
                    Ok(PresenceUpdate::Set { state, details }) => {
                        last = Some((state.clone(), details.clone()));
                        if let Some(c) = client.as_mut() {
                            let res = c.set_activity(
                                activity::Activity::new().state(&state).details(&details),
                            );
                            if res.is_err() {
                                client = None; // Discord closed, reconnect next loop
                            }
                        }
                    }
                    Ok(PresenceUpdate::Clear) => {
                        last = None;
                        if let Some(c) = client.as_mut() {
                            let _ = c.clear_activity();
                        }
                    }
                    Err(mpsc::RecvTimeoutError::Timeout) => {}
                    Err(mpsc::RecvTimeoutError::Disconnected) => break, // app exiting
                }
            }
        });

        Self { tx }
    }

    pub fn set(&self, state: impl Into<String>, details: impl Into<String>) {
        let _ = self.tx.send(PresenceUpdate::Set {
            state: state.into(),
            details: details.into(),
        });
    }
}
