use std::sync::mpsc::{self, Receiver};
use std::thread;
use std::time::Duration;

use serde::Deserialize;

const GITHUB_CLIENT_ID: &str = "Ov23liXDVfP3LF16SG7C";

// no github secret needed

#[derive(Deserialize, Clone)]
pub struct DeviceCode {
    pub device_code: String,
    pub user_code: String,
    pub verification_uri: String,
    pub expires_in: u64,
    pub interval: u64,
}

#[derive(Deserialize)]
struct AccessTokenResponse {
    access_token: Option<String>,
    error: Option<String>,
}

pub enum SignInEvent {
    ShowCode(DeviceCode),
    Success(String),
    Failed(String),
}

fn request_device_code(client: &reqwest::blocking::Client) -> Result<DeviceCode, String> {
    let response = client
        .post("https://github.com/login/device/code")
        .header("Accept", "application/json")
        .form(&[("client_id", GITHUB_CLIENT_ID), ("scope", "repo")])
        .send()
        .map_err(|e| e.to_string())?;

    response.json::<DeviceCode>().map_err(|e| e.to_string())
}

fn poll_for_token(
    client: &reqwest::blocking::Client,
    device_code: &str,
    interval: u64,
) -> Result<String, String> {
    let mut wait = Duration::from_secs(interval);

    loop {
        thread::sleep(wait);

        let response = client
            .post("https://github.com/login/oauth/access_token")
            .header("Accept", "application/json")
            .form(&[
                ("client_id", GITHUB_CLIENT_ID),
                ("device_code", device_code),
                ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
            ])
            .send()
            .map_err(|e| e.to_string())?;

        let parsed: AccessTokenResponse = response.json().map_err(|e| e.to_string())?;

        if let Some(token) = parsed.access_token {
            return Ok(token);
        }

        match parsed.error.as_deref() {
            Some("authorization_pending") => continue,
            Some("slow_down") => {
                wait += Duration::from_secs(5);
                continue;
            }
            Some("expired_token") => return Err("The sign-in code expired. Try again.".into()),
            Some("access_denied") => return Err("Sign-in was denied.".into()),
            Some(other) => return Err(format!("GitHub returned an error: {other}")),
            None => return Err("Unexpected response from GitHub.".into()),
        }
    }
}

pub fn start_sign_in() -> Receiver<SignInEvent> {
    let (tx, rx) = mpsc::channel();

    thread::spawn(move || {
        let client = match reqwest::blocking::Client::builder().build() {
            Ok(c) => c,
            Err(e) => {
                let _ = tx.send(SignInEvent::Failed(e.to_string()));
                return;
            }
        };

        let device_code = match request_device_code(&client) {
            Ok(dc) => dc,
            Err(e) => {
                let _ = tx.send(SignInEvent::Failed(e));
                return;
            }
        };

        let _ = tx.send(SignInEvent::ShowCode(device_code.clone()));

        match poll_for_token(&client, &device_code.device_code, device_code.interval) {
            Ok(token) => {
                let _ = save_token(&token);
                let _ = tx.send(SignInEvent::Success(token));
            }
            Err(e) => {
                let _ = tx.send(SignInEvent::Failed(e));
            }
        }
    });

    rx
}

pub struct RepoSummary {
    pub full_name: String,
    pub clone_url: String,
    pub private: bool,
}

pub fn list_repos_async(token: String) -> Receiver<Result<Vec<RepoSummary>, String>> {
    let (tx, rx) = mpsc::channel();

    thread::spawn(move || {
        let result = (|| -> Result<Vec<RepoSummary>, String> {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .map_err(|e| e.to_string())?;

            rt.block_on(async {
                let octocrab = octocrab::Octocrab::builder()
                    .personal_token(token)
                    .build()
                    .map_err(|e| e.to_string())?;

                let page = octocrab
                    .current()
                    .list_repos_for_authenticated_user()
                    .sort("updated")
                    .per_page(50)
                    .send()
                    .await
                    .map_err(|e| e.to_string())?;

                Ok(page
                    .items
                    .into_iter()
                    .filter_map(|repo| {
                        Some(RepoSummary {
                            full_name: repo.full_name?,
                            clone_url: repo.clone_url?.to_string(),
                            private: repo.private.unwrap_or(false),
                        })
                    })
                    .collect())
            })
        })();

        let _ = tx.send(result);
    });

    rx
}

const KEYRING_SERVICE: &str = "top.actinium.actinium";
const KEYRING_USER: &str = "github";

pub fn save_token(token: &str) -> Result<(), String> {
    let entry = keyring::Entry::new(KEYRING_SERVICE, KEYRING_USER).map_err(|e| e.to_string())?;
    entry.set_password(token).map_err(|e| e.to_string())
}

pub fn load_token() -> Option<String> {
    let entry = keyring::Entry::new(KEYRING_SERVICE, KEYRING_USER).ok()?;
    entry.get_password().ok()
}

pub fn sign_out() -> Result<(), String> {
    let entry = keyring::Entry::new(KEYRING_SERVICE, KEYRING_USER).map_err(|e| e.to_string())?;
    entry.delete_credential().map_err(|e| e.to_string())
}
