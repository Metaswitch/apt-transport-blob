// Copyright (c) Alianza, Inc. All rights reserved.
// Licensed under the MIT License.
use log::{debug, warn};
use url::Url;

const STORAGE_SCOPE: &str = "https://storage.azure.com/.default";
const STORAGE_RESOURCE: &str = "https://storage.azure.com";
const DEFAULT_AUTHORITY_HOST: &str = "login.microsoftonline.com";
const IMDS_ENDPOINT: &str = "http://169.254.169.254/metadata/identity/oauth2/token";

const MAX_RETRIES: u32 = 3;
const RETRY_STATUSES: [u16; 6] = [408, 429, 500, 502, 503, 504];

#[derive(Clone)]
struct RetryClient {
    inner: reqwest::Client,
}

impl RetryClient {
    fn new() -> Self {
        Self {
            inner: reqwest::Client::new(),
        }
    }

    fn get(&self, url: &str) -> RetryRequestBuilder {
        RetryRequestBuilder(self.inner.get(url))
    }

    fn head(&self, url: &str) -> RetryRequestBuilder {
        RetryRequestBuilder(self.inner.head(url))
    }

    fn post(&self, url: String) -> RetryRequestBuilder {
        RetryRequestBuilder(self.inner.post(url))
    }
}

struct RetryRequestBuilder(reqwest::RequestBuilder);

impl RetryRequestBuilder {
    fn header(self, key: &str, value: &str) -> Self {
        Self(self.0.header(key, value))
    }

    fn form<T: serde::Serialize + ?Sized>(self, form: &T) -> Self {
        Self(self.0.form(form))
    }

    fn query<T: serde::Serialize + ?Sized>(self, query: &T) -> Self {
        Self(self.0.query(query))
    }

    fn timeout(self, timeout: std::time::Duration) -> Self {
        Self(self.0.timeout(timeout))
    }

    async fn send(self) -> Result<reqwest::Response, Box<dyn std::error::Error>> {
        for attempt in 0..MAX_RETRIES {
            let cloned = self.0.try_clone().ok_or("Request body is not cloneable")?;
            match cloned.send().await {
                Ok(resp) if RETRY_STATUSES.contains(&resp.status().as_u16()) => {
                    let status = resp.status();
                    let delay = if status.as_u16() == 429 {
                        resp.headers()
                            .get("retry-after")
                            .and_then(|v| v.to_str().ok())
                            .and_then(|v| v.parse::<u64>().ok())
                            .map(std::time::Duration::from_secs)
                    } else {
                        None
                    }
                    .unwrap_or_else(|| std::time::Duration::from_millis(500 * 2u64.pow(attempt)));
                    warn!(
                        "Retryable status {status}, attempt {attempt}/{MAX_RETRIES}, retrying in {delay:?}"
                    );
                    tokio::time::sleep(delay).await;
                }
                Ok(resp) => return Ok(resp),
                Err(e) if e.is_timeout() || e.is_connect() => {
                    let delay = std::time::Duration::from_millis(500 * 2u64.pow(attempt));
                    warn!(
                        "Retryable error: {e}, attempt {attempt}/{MAX_RETRIES}, retrying in {delay:?}"
                    );
                    tokio::time::sleep(delay).await;
                }
                Err(e) => return Err(e.into()),
            }
        }
        Ok(self.0.send().await?)
    }
}

fn extract_access_token(json: &serde_json::Value) -> Result<String, Box<dyn std::error::Error>> {
    json.get("access_token")
        .or_else(|| json.get("accessToken"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .ok_or_else(|| "Missing access_token in response".into())
}

fn aad_token_url(tenant_id: &str) -> String {
    let authority = std::env::var("AZURE_AUTHORITY_HOST")
        .unwrap_or_else(|_| DEFAULT_AUTHORITY_HOST.to_string());
    format!("https://{authority}/{tenant_id}/oauth2/v2.0/token")
}

async fn try_workload_identity(client: &RetryClient) -> Result<String, Box<dyn std::error::Error>> {
    let tenant_id = std::env::var("AZURE_TENANT_ID")?;
    let client_id = std::env::var("AZURE_CLIENT_ID")?;
    let token_file = std::env::var("AZURE_FEDERATED_TOKEN_FILE")?;

    let assertion = tokio::fs::read_to_string(&token_file).await?;

    debug!("Trying WorkloadIdentityCredential");
    let response = client
        .post(aad_token_url(&tenant_id))
        .form(&[
            ("client_id", client_id.as_str()),
            ("client_assertion", assertion.trim()),
            (
                "client_assertion_type",
                "urn:ietf:params:oauth:client-assertion-type:jwt-bearer",
            ),
            ("grant_type", "client_credentials"),
            ("scope", STORAGE_SCOPE),
        ])
        .send()
        .await?;

    if !response.status().is_success() {
        let body = response.text().await.unwrap_or_default();
        return Err(format!("WorkloadIdentity auth failed: {body}").into());
    }

    let json: serde_json::Value = response.json().await?;
    extract_access_token(&json)
}

async fn try_client_secret(client: &RetryClient) -> Result<String, Box<dyn std::error::Error>> {
    let tenant_id = std::env::var("AZURE_TENANT_ID")?;
    let client_id = std::env::var("AZURE_CLIENT_ID")?;
    let client_secret = std::env::var("AZURE_CLIENT_SECRET")?;

    debug!("Trying ClientSecretCredential");
    let response = client
        .post(aad_token_url(&tenant_id))
        .form(&[
            ("client_id", client_id.as_str()),
            ("client_secret", client_secret.as_str()),
            ("grant_type", "client_credentials"),
            ("scope", STORAGE_SCOPE),
        ])
        .send()
        .await?;

    if !response.status().is_success() {
        let body = response.text().await.unwrap_or_default();
        return Err(format!("ClientSecret auth failed: {body}").into());
    }

    let json: serde_json::Value = response.json().await?;
    extract_access_token(&json)
}

async fn try_managed_identity(client: &RetryClient) -> Result<String, Box<dyn std::error::Error>> {
    debug!("Trying ManagedIdentityCredential");
    let response = client
        .get(IMDS_ENDPOINT)
        .query(&[
            ("api-version", "2018-02-01"),
            ("resource", STORAGE_RESOURCE),
        ])
        .header("Metadata", "true")
        .timeout(std::time::Duration::from_secs(2))
        .send()
        .await?;

    if !response.status().is_success() {
        let body = response.text().await.unwrap_or_default();
        return Err(format!("ManagedIdentity auth failed: {body}").into());
    }

    let json: serde_json::Value = response.json().await?;
    extract_access_token(&json)
}

async fn try_az_cli() -> Result<String, Box<dyn std::error::Error>> {
    debug!("Trying DeveloperToolsCredential (az CLI)");
    let output = tokio::process::Command::new("az")
        .args([
            "account",
            "get-access-token",
            "--resource",
            STORAGE_RESOURCE,
            "--output",
            "json",
        ])
        .output()
        .await?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("az CLI failed: {stderr}").into());
    }

    let json: serde_json::Value = serde_json::from_slice(&output.stdout)?;
    extract_access_token(&json)
}

async fn obtain_token(client: &RetryClient) -> Result<String, Box<dyn std::error::Error>> {
    if let Ok(token) = std::env::var("AZURE_STORAGE_BEARER_TOKEN") {
        debug!("Using AZURE_STORAGE_BEARER_TOKEN environment variable");
        return Ok(token);
    }

    match try_workload_identity(client).await {
        Ok(token) => {
            debug!("Using WorkloadIdentityCredential");
            return Ok(token);
        }
        Err(e) => warn!("WorkloadIdentityCredential failed: {e}"),
    }

    match try_client_secret(client).await {
        Ok(token) => {
            debug!("Using ClientSecretCredential");
            return Ok(token);
        }
        Err(e) => warn!("ClientSecretCredential failed: {e}"),
    }

    match try_az_cli().await {
        Ok(token) => {
            debug!("Using DeveloperToolsCredential (az CLI)");
            return Ok(token);
        }
        Err(e) => warn!("DeveloperToolsCredential (az CLI) failed: {e}"),
    }

    match try_managed_identity(client).await {
        Ok(token) => {
            debug!("Using ManagedIdentityCredential");
            return Ok(token);
        }
        Err(e) => warn!("ManagedIdentityCredential failed: {e}"),
    }

    Err("No suitable credential found".into())
}

pub struct AzureBlob {
    client: RetryClient,
    url: String,
    token: String,
}

impl std::fmt::Debug for AzureBlob {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AzureBlob")
            .field("url", &self.url)
            .field("token", &"[REDACTED]")
            .finish()
    }
}

impl AzureBlob {
    pub fn new_from_url(
        azure_registry: &AzureRegistry,
        url: &Url,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let https_url = format!(
            "https://{}{}",
            url.host_str().ok_or("No host in URL")?,
            url.path()
        );
        Ok(AzureBlob {
            client: azure_registry.client.clone(),
            url: https_url,
            token: azure_registry.token.clone(),
        })
    }

    fn auth_header(&self) -> String {
        format!("Bearer {}", self.token)
    }

    pub async fn exists(&self) -> Result<bool, Box<dyn std::error::Error>> {
        let auth = self.auth_header();
        let response = self
            .client
            .head(&self.url)
            .header("Authorization", &auth)
            .header("x-ms-version", "2020-04-08")
            .send()
            .await?;

        match response.status().as_u16() {
            200 => Ok(true),
            404 => Ok(false),
            status => Err(format!("Unexpected status {status} checking blob existence").into()),
        }
    }

    pub async fn uri_start_fields(&self) -> Result<(u64, String), Box<dyn std::error::Error>> {
        let auth = self.auth_header();
        let response = self
            .client
            .head(&self.url)
            .header("Authorization", &auth)
            .header("x-ms-version", "2020-04-08")
            .send()
            .await?;

        if !response.status().is_success() {
            return Err(format!("Failed to get blob properties: {}", response.status()).into());
        }

        let content_length = response
            .headers()
            .get("Content-Length")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.parse::<u64>().ok())
            .ok_or("Missing or invalid Content-Length header")?;

        let last_modified = response
            .headers()
            .get("Last-Modified")
            .and_then(|v| v.to_str().ok())
            .ok_or("Missing Last-Modified header")?
            .to_string();

        Ok((content_length, last_modified))
    }

    pub(crate) async fn download(&self) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
        let auth = self.auth_header();
        let response = self
            .client
            .get(&self.url)
            .header("Authorization", &auth)
            .header("x-ms-version", "2020-04-08")
            .send()
            .await?;

        if !response.status().is_success() {
            return Err(format!("Failed to download blob: {}", response.status()).into());
        }

        Ok(response.bytes().await?.to_vec())
    }
}

pub(crate) struct AzureRegistry {
    client: RetryClient,
    token: String,
}

impl AzureRegistry {
    pub async fn new() -> Result<Self, Box<dyn std::error::Error>> {
        let client = RetryClient::new();
        let token = obtain_token(&client).await?;
        Ok(AzureRegistry { client, token })
    }

    pub fn get_blob(&self, url: &Url) -> Result<AzureBlob, Box<dyn std::error::Error>> {
        AzureBlob::new_from_url(self, url)
    }
}
