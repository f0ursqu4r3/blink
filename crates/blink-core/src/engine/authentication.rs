//! Native public-client OAuth (S256 PKCE), refresh, and Keychain operations.
use super::Engine;
use crate::credentials;
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct OAuthConfig {
    pub authorization_url: String,
    pub token_url: String,
    pub client_id: String,
    pub scopes: String,
}
#[derive(Serialize, Deserialize)]
struct OAuthCredential {
    config: OAuthConfig,
    access_token: String,
    refresh_token: Option<String>,
    expires_at: Option<u64>,
}
/// Contains the bound listener; dropping this cancels an unstarted login.
pub struct OAuthLogin {
    pub authorization_url: String,
    listener: tokio::net::TcpListener,
    state: String,
    verifier: String,
    redirect: String,
    config: OAuthConfig,
}
fn random_token() -> String {
    let mut bytes = [0; 32];
    rand::rng().fill_bytes(&mut bytes);
    URL_SAFE_NO_PAD.encode(bytes)
}
pub fn challenge(verifier: &str) -> String {
    URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()))
}
fn endpoint(value: &str) -> Result<url::Url, String> {
    let url = url::Url::parse(value).map_err(|_| "Invalid OAuth endpoint.")?;
    if url.scheme() != "https"
        || !url.username().is_empty()
        || url.password().is_some()
        || url.fragment().is_some()
    {
        return Err("OAuth endpoints must use HTTPS without credentials or fragments.".into());
    }
    Ok(url)
}
fn callback(target: &str, state: &str) -> Result<String, String> {
    if !target.starts_with("/oauth/callback?") {
        return Err("Invalid OAuth callback path.".into());
    }
    let url = url::Url::parse(&format!("http://127.0.0.1{target}"))
        .map_err(|_| "Invalid OAuth callback.")?;
    let params: Vec<_> = url.query_pairs().collect();
    let values = |key: &str| {
        params
            .iter()
            .filter(|(k, _)| k == key)
            .map(|(_, v)| v.as_ref())
            .collect::<Vec<_>>()
    };
    if values("state") != [state] {
        return Err("OAuth state does not match. Start sign-in again.".into());
    }
    if !values("error").is_empty() {
        return Err("The authorization server declined sign-in.".into());
    }
    let code = values("code");
    if code.len() != 1 || code[0].is_empty() {
        return Err("The OAuth callback has no unique code.".into());
    }
    Ok(code[0].to_owned())
}
fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
async fn exchange(
    config: OAuthConfig,
    fields: Vec<(&str, String)>,
    previous_refresh: Option<String>,
) -> Result<OAuthCredential, String> {
    endpoint(&config.token_url)?;
    let builder = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .redirect(reqwest::redirect::Policy::none());
    #[cfg(test)]
    let builder = super::auth_test_support::trust_fixture(builder);
    let client = builder.build().map_err(|_| "Cannot start OAuth client.")?;
    let mut response = client
        .post(&config.token_url)
        .form(&fields)
        .send()
        .await
        .map_err(|_| "OAuth token exchange failed.")?;
    if !response.status().is_success() {
        return Err(format!(
            "OAuth token endpoint returned HTTP {}.",
            response.status().as_u16()
        ));
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| "Cannot read OAuth response.")?
    {
        if bytes.len() + chunk.len() > 1024 * 1024 {
            return Err("OAuth response exceeds 1 MiB.".into());
        }
        bytes.extend_from_slice(&chunk);
    }
    let json: serde_json::Value =
        serde_json::from_slice(&bytes).map_err(|_| "Invalid OAuth token response.")?;
    if !json["token_type"]
        .as_str()
        .is_some_and(|s| s.eq_ignore_ascii_case("bearer"))
    {
        return Err("OAuth requires a Bearer token response.".into());
    }
    let access_token = json["access_token"]
        .as_str()
        .filter(|s| !s.is_empty())
        .ok_or("OAuth response has no access token.")?
        .to_owned();
    let refresh_token = json["refresh_token"]
        .as_str()
        .map(str::to_owned)
        .or(previous_refresh);
    let expires_at = json["expires_in"]
        .as_u64()
        .map(|seconds| now().saturating_add(seconds));
    Ok(OAuthCredential {
        config,
        access_token,
        refresh_token,
        expires_at,
    })
}
async fn lookup(scope: Option<&str>, name: &str) -> Result<String, String> {
    if let Some(bytes) = credentials::get(scope, &format!("oauth:{name}"))? {
        let mut credential: OAuthCredential = serde_json::from_slice(&bytes)
            .map_err(|_| "Invalid saved OAuth credential. Sign in again.")?;
        if credential.expires_at.is_some_and(|at| at <= now() + 30) {
            let refresh = credential
                .refresh_token
                .clone()
                .ok_or("OAuth token expired. Sign in again.")?;
            let fields = vec![
                ("grant_type", "refresh_token".into()),
                ("client_id", credential.config.client_id.clone()),
                ("refresh_token", refresh.clone()),
            ];
            credential = exchange(credential.config, fields, Some(refresh)).await?;
            credentials::set(
                scope,
                &format!("oauth:{name}"),
                &serde_json::to_vec(&credential).map_err(|_| "Cannot encode OAuth credential.")?,
            )?;
        }
        return Ok(credential.access_token);
    }
    let bytes = credentials::get(scope, name)?
        .ok_or_else(|| format!("Secret \"{name}\" is not stored for this project."))?;
    String::from_utf8(bytes).map_err(|_| "This credential is not text.".into())
}

pub(super) async fn resolve(
    request: &mut crate::model::RequestInput,
    scope: Option<&str>,
) -> Result<Vec<String>, String> {
    // Extract from decoded Basic auth too. Resolve each name once per send.
    let mut texts = vec![
        request.url.clone(),
        request.body.clone().unwrap_or_default(),
    ];
    for header in &request.headers {
        texts.push(header.value.clone());
        if header.key.eq_ignore_ascii_case("authorization")
            && let Some(encoded) = header.value.strip_prefix("Basic ")
        {
            use base64::engine::general_purpose::STANDARD;
            if let Ok(bytes) = STANDARD.decode(encoded)
                && let Ok(text) = String::from_utf8(bytes)
            {
                texts.push(text);
            }
        }
    }
    for part in request.multipart.iter().flatten() {
        texts.push(part.key.clone());
        if !part.file {
            texts.push(part.value.clone());
        }
    }
    let mut resolved = std::collections::HashMap::new();
    for text in texts {
        for name in credentials::references(&text) {
            if !resolved.contains_key(&name) {
                resolved.insert(name.clone(), lookup(scope, &name).await?);
            }
        }
    }
    credentials::resolve_request(request, |name| {
        resolved
            .get(name)
            .cloned()
            .ok_or_else(|| "Invalid secret reference.".into())
    })?;
    Ok(resolved.into_values().collect())
}
impl Engine {
    pub fn save_secret(
        &self,
        scope: Option<String>,
        name: String,
        value: String,
    ) -> impl Future<Output = Result<(), String>> + Send + 'static {
        let engine = self.clone();
        self.spawn(
            async move {
                let _guard = engine.0.credential_lock.lock().await;
                engine
                    .blocking(
                        move || {
                            if !credentials::valid_name(&name) || value.is_empty() {
                                return Err("Enter a valid name and a nonempty secret.".into());
                            }
                            credentials::set(scope.as_deref(), &name, value.as_bytes())?;
                            credentials::delete(scope.as_deref(), &format!("oauth:{name}"))
                        },
                        "Cannot save secret.",
                    )
                    .await
            },
            "Cannot save secret.",
        )
    }
    pub fn remove_secret(
        &self,
        scope: Option<String>,
        name: String,
    ) -> impl Future<Output = Result<(), String>> + Send + 'static {
        let engine = self.clone();
        self.spawn(
            async move {
                #[cfg(test)]
                super::auth_test_support::credential_waiting(scope.as_deref(), "remove");
                let _guard = engine.0.credential_lock.lock().await;
                engine
                    .blocking(
                        move || {
                            if !credentials::valid_name(&name) {
                                return Err("Enter a valid secret name.".into());
                            }
                            credentials::delete(scope.as_deref(), &format!("oauth:{name}"))?;
                            credentials::delete(scope.as_deref(), &name)
                        },
                        "Cannot remove secret.",
                    )
                    .await
            },
            "Cannot remove secret.",
        )
    }
    pub fn begin_oauth(
        &self,
        config: OAuthConfig,
    ) -> impl Future<Output = Result<OAuthLogin, String>> + Send + 'static {
        self.spawn(
            async move {
                let mut auth = endpoint(&config.authorization_url)?;
                endpoint(&config.token_url)?;
                if config.client_id.trim().is_empty() {
                    return Err("Enter an OAuth client ID.".into());
                }
                let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
                    .await
                    .map_err(|_| "Cannot bind OAuth callback listener.")?;
                let redirect = format!(
                    "http://127.0.0.1:{}/oauth/callback",
                    listener
                        .local_addr()
                        .map_err(|_| "Cannot read callback port.")?
                        .port()
                );
                let state = random_token();
                let verifier = random_token();
                // Reject conflicting endpoint query parameters instead of sending duplicates.
                if auth.query_pairs().any(|(key, _)| {
                    [
                        "response_type",
                        "client_id",
                        "redirect_uri",
                        "scope",
                        "state",
                        "code_challenge",
                        "code_challenge_method",
                    ]
                    .contains(&key.as_ref())
                }) {
                    return Err(
                        "Remove OAuth protocol parameters from the authorization endpoint URL."
                            .into(),
                    );
                }
                auth.query_pairs_mut().extend_pairs([
                    ("response_type", "code"),
                    ("client_id", &config.client_id),
                    ("redirect_uri", &redirect),
                    ("scope", &config.scopes),
                    ("state", &state),
                    ("code_challenge", &challenge(&verifier)),
                    ("code_challenge_method", "S256"),
                ]);
                Ok(OAuthLogin {
                    authorization_url: auth.into(),
                    listener,
                    state,
                    verifier,
                    redirect,
                    config,
                })
            },
            "Cannot start OAuth sign-in.",
        )
    }
    pub fn finish_oauth(
        &self,
        login: OAuthLogin,
        scope: Option<String>,
        name: String,
    ) -> impl Future<Output = Result<(), String>> + Send + 'static {
        let engine = self.clone();
        self.spawn(async move {
            if !credentials::valid_name(&name) {return Err("Enter a valid secret name.".into());}
            let flow=async {
                loop {
                    let (mut socket,_)=login.listener.accept().await.map_err(|_| "Cannot receive OAuth callback.")?;
                    let read=async {
                        let mut bytes=Vec::new();let mut buf=[0;1024];
                        loop {let count=socket.read(&mut buf).await.map_err(|_| "Cannot read OAuth callback.")?;
                            if count==0 {return Err("Incomplete OAuth callback.");} bytes.extend_from_slice(&buf[..count]);
                            if bytes.len()>16384 {return Err("OAuth callback is too large.");}
                            if bytes.windows(4).any(|w|w==b"\r\n\r\n") {break;}
                        }
                        String::from_utf8(bytes).map_err(|_|"Invalid OAuth callback.")
                    };
                    let Ok(Ok(request))=tokio::time::timeout(Duration::from_secs(5),read).await else {continue};
                    let mut parts=request.lines().next().unwrap_or_default().split_whitespace();
                    let method=parts.next();let target=parts.next().unwrap_or_default();
                    let result=if method==Some("GET") {callback(target,&login.state)} else {Err("OAuth callback must use GET.".into())};
                    let success=result.is_ok();
                    let message=if success {"Sign-in received. Return to Blink."} else {"Invalid callback. Return to Blink and try again."};
                    let reply=format!("HTTP/1.1 {}\r\nContent-Type: text/plain\r\nCache-Control: no-store\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",if success {"200 OK"} else {"400 Bad Request"},message.len(),message);
                    let _=socket.write_all(reply.as_bytes()).await;
                    let code = match result {
                        Ok(code) => code,
                        Err(message) if message == "The authorization server declined sign-in." => return Err(message),
                        Err(_) => continue,
                    };
                    let fields=vec![("grant_type","authorization_code".into()),("client_id",login.config.client_id.clone()),("redirect_uri",login.redirect.clone()),("code_verifier",login.verifier.clone()),("code",code)];
                    let credential=exchange(login.config,fields,None).await?;
                    let _guard = engine.0.credential_lock.lock().await;
                    credentials::set(scope.as_deref(),&format!("oauth:{name}"),&serde_json::to_vec(&credential).map_err(|_|"Cannot encode OAuth credential.")?)?;
                    return Ok(());
                }
            };
            tokio::time::timeout(Duration::from_secs(180),flow).await.map_err(|_|"OAuth sign-in timed out. Start again.".to_string())?
        },"OAuth sign-in failed.")
    }
    pub fn save_client_identity(
        &self,
        scope: Option<String>,
        origin: String,
        path: std::path::PathBuf,
    ) -> impl Future<Output = Result<(), String>> + Send + 'static {
        self.blocking(
            move || {
                let key = credentials::identity_key(&origin)?;
                let size = std::fs::metadata(&path)
                    .map_err(|_| "Cannot read client identity file.")?
                    .len();
                if size > 1024 * 1024 {
                    return Err("Client identity exceeds 1 MiB.".into());
                }
                let pem = std::fs::read(path).map_err(|_| "Cannot read client identity file.")?;
                reqwest::Identity::from_pem(&pem)
                    .map_err(|_| "Choose a PEM file with its certificate chain and private key.")?;
                credentials::set(scope.as_deref(), &key, &pem)
            },
            "Cannot save client certificate.",
        )
    }
    pub fn remove_client_identity(
        &self,
        scope: Option<String>,
        origin: String,
    ) -> impl Future<Output = Result<(), String>> + Send + 'static {
        self.blocking(
            move || credentials::delete(scope.as_deref(), &credentials::identity_key(&origin)?),
            "Cannot remove client certificate.",
        )
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rfc7636_vector() {
        assert_eq!(
            challenge("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk"),
            "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
        );
    }
    #[test]
    fn callback_checks_path_state_and_duplicate_values() {
        assert_eq!(
            callback("/oauth/callback?code=a%2Bb&state=expected", "expected").unwrap(),
            "a+b"
        );
        for query in [
            "/wrong?code=a&state=s",
            "/oauth/callback?code=a&state=bad",
            "/oauth/callback?code=a&state=s&state=s",
            "/oauth/callback?code=a&code=b&state=s",
            "/oauth/callback?error=denied&state=s",
        ] {
            assert!(callback(query, "s").is_err());
        }
    }
    #[test]
    fn validated_provider_denial_finishes_without_waiting_for_timeout() {
        use std::io::{Read, Write};
        let directory = tempfile::tempdir().unwrap();
        let engine = Engine::new(super::super::Paths::with_base(
            directory.path().to_path_buf(),
        ))
        .unwrap();
        let login = futures::executor::block_on(engine.begin_oauth(OAuthConfig {
            authorization_url: "https://example.test/auth".into(),
            token_url: "https://example.test/token".into(),
            client_id: "test".into(),
            scopes: String::new(),
        }))
        .unwrap();
        let redirect = url::Url::parse(&login.redirect).unwrap();
        let target = format!(
            "{}?error=access_denied&state={}",
            redirect.path(),
            login.state
        );
        let address = format!("127.0.0.1:{}", redirect.port().unwrap());
        let finish = engine.finish_oauth(login, None, "test".into());
        let mut socket = std::net::TcpStream::connect(&address).unwrap();
        socket
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        write!(socket, "GET {target} HTTP/1.1\r\nHost: {address}\r\n\r\n").unwrap();
        let mut response = String::new();
        socket.read_to_string(&mut response).unwrap();
        assert!(response.contains("400 Bad Request"));
        assert_eq!(
            futures::executor::block_on(finish).unwrap_err(),
            "The authorization server declined sign-in."
        );
    }
    #[test]
    fn endpoints_require_tls() {
        assert!(endpoint("http://example.com/token").is_err());
        assert!(endpoint("https://user:pass@example.com").is_err());
    }
}

#[cfg(test)]
mod integration_tests;
