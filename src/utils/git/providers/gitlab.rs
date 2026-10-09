use std::sync::mpsc::{self, Receiver};
use std::thread;
use std::time::Duration;

use serde::Deserialize;
use gitlab::api::Query;

const GITLAB_CLIENT_ID: &str = "1b2961139642b9500cafc14f24027b9ade5b91491e2ecd60a97e56cccfea5444";

#[derive(Deserialize, Clone)]
pub struct DeviceCode {
    pub device_code: String,
    pub user_code: String,
    pub verification_uri: String,
    pub verification_uri_complete: Option<String>,
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

fn request_device_code(
    client: &reqwest::blocking::Client,
    base_url: &str,
) -> Result<DeviceCode, String> {
    let response = client
        .post(format!("{base_url}/oauth/authorize_device"))
        .header("Accept", "application/json")
        .form(&[
            ("client_id", GITLAB_CLIENT_ID),
            ("scope", "read_repository write_repository"),
        ])
        .send()
        .map_err(|e| e.to_string())?;

    if !response.status().is_success() {
        return Err(format!(
            "GitLab returned {} — this instance may not support device flow (requires GitLab 17.1+)",
            response.status()
        ));
    }

    response.json::<DeviceCode>().map_err(|e| e.to_string())
}

fn poll_for_token(
    client: &reqwest::blocking::Client,
    base_url: &str,
    device_code: &str,
    interval: u64,
) -> Result<String, String> {
    let mut wait = Duration::from_secs(interval);

    loop {
        thread::sleep(wait);

        let response = client
            .post(format!("{base_url}/oauth/token"))
            .header("Accept", "application/json")
            .form(&[
                ("client_id", GITLAB_CLIENT_ID),
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
            Some(other) => return Err(format!("GitLab returned an error: {other}")),
            None => return Err("Unexpected response from GitLab.".into()),
        }
    }
}

pub fn start_sign_in(base_url: String) -> Receiver<SignInEvent> {
    let (tx, rx) = mpsc::channel();

    thread::spawn(move || {
        let client = match reqwest::blocking::Client::builder().build() {
            Ok(c) => c,
            Err(e) => {
                let _ = tx.send(SignInEvent::Failed(e.to_string()));
                return;
            }
        };

        let device_code = match request_device_code(&client, &base_url) {
            Ok(dc) => dc,
            Err(e) => {
                let _ = tx.send(SignInEvent::Failed(e));
                return;
            }
        };

        let _ = tx.send(SignInEvent::ShowCode(device_code.clone()));

        match poll_for_token(
            &client,
            &base_url,
            &device_code.device_code,
            device_code.interval,
        ) {
            Ok(token) => {
                let _ = save_token(&base_url, &token);
                let _ = tx.send(SignInEvent::Success(token));
            }
            Err(e) => {
                let _ = tx.send(SignInEvent::Failed(e));
            }
        }
    });

    rx
}

pub struct ProjectSummary {
    pub path_with_namespace: String,
    pub http_url_to_repo: String,
    pub visibility_is_private: bool,
}

pub fn list_projects_async(
    base_url: String,
    token: String,
) -> Receiver<Result<Vec<ProjectSummary>, String>> {
    let (tx, rx) = mpsc::channel();

    thread::spawn(move || {
        let result = (|| -> Result<Vec<ProjectSummary>, String> {
            let host = base_url
                .trim_start_matches("https://")
                .trim_start_matches("http://");

            let client = gitlab::Gitlab::new(host, &token).map_err(|e| e.to_string())?;

            let endpoint = gitlab::api::projects::Projects::builder()
                .membership(true)
                .build()
                .map_err(|e| e.to_string())?;

            #[derive(serde::Deserialize)]
            struct RawProject {
                path_with_namespace: String,
                http_url_to_repo: String,
                visibility: String,
            }

            let projects: Vec<RawProject> =
                gitlab::api::paged(endpoint, gitlab::api::Pagination::Limit(50))
                    .query(&client)
                    .map_err(|e| e.to_string())?;

            Ok(projects
                .into_iter()
                .map(|p| ProjectSummary {
                    path_with_namespace: p.path_with_namespace,
                    http_url_to_repo: p.http_url_to_repo,
                    visibility_is_private: p.visibility == "private",
                })
                .collect())
        })();

        let _ = tx.send(result);
    });

    rx
}

fn keyring_user(base_url: &str) -> String {
    format!("gitlab:{base_url}")
}

pub fn save_token(base_url: &str, token: &str) -> Result<(), String> {
    let entry = keyring::Entry::new("top.actinium.actinium", &keyring_user(base_url))
        .map_err(|e| e.to_string())?;
    entry.set_password(token).map_err(|e| e.to_string())
}

pub fn load_token(base_url: &str) -> Option<String> {
    let entry = keyring::Entry::new("top.actinium.actinium", &keyring_user(base_url)).ok()?;
    entry.get_password().ok()
}

pub fn sign_out(base_url: &str) -> Result<(), String> {
    let entry = keyring::Entry::new("top.actinium.actinium", &keyring_user(base_url))
        .map_err(|e| e.to_string())?;
    entry.delete_credential().map_err(|e| e.to_string())
}
