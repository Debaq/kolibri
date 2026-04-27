// OAuth2 PKCE flow + keyring storage para tokens.
//
// - `run_loopback_pkce`: abre navegador, escucha en 127.0.0.1:PORT, exchange
//   code → tokens. Reusable por Gmail (con secret) y Microsoft Graph (sin
//   secret, public client).
// - `refresh_access`: renueva access_token con refresh_token.
// - `keyring_*`: guarda/lee TokenSet completo del keyring del SO. Service
//   "kolibri", account "{kind}:{service_id}". Nada de tokens en services.json.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use oauth2::basic::BasicClient;
use oauth2::reqwest::async_http_client;
use oauth2::{
    AuthUrl, AuthorizationCode, ClientId, ClientSecret, CsrfToken, PkceCodeChallenge,
    RedirectUrl, RefreshToken, Scope, TokenResponse, TokenUrl,
};
use serde::{Deserialize, Serialize};

const KEYRING_SERVICE: &str = "kolibri";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenSet {
    pub access_token: String,
    pub refresh_token: String,
    pub expires_at: i64,
    pub scope: String,
}

#[derive(Debug, Clone)]
pub struct AuthResult {
    pub tokens: TokenSet,
    /// Email del usuario autenticado (Gmail userinfo / Graph /me).
    pub email: String,
}

fn now_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Corre el flow PKCE completo:
///   1. Levanta listener loopback en 127.0.0.1:PORT (port elegido por el SO).
///   2. Construye URL de auth con code_challenge S256.
///   3. Abre navegador del SO.
///   4. Espera callback con `?code=...&state=...`.
///   5. Exchange code → token set (con refresh_token).
///   6. (Opcional) Llama `userinfo_url` con bearer para obtener email.
///
/// `client_secret = None` → public client (Graph PowerShell). PKCE igual aplica.
/// `extra_params` para Google: `[("access_type","offline"),("prompt","consent")]`
/// para forzar siempre devolver refresh_token.
pub async fn run_loopback_pkce(
    auth_url: &str,
    token_url: &str,
    client_id: &str,
    client_secret: Option<&str>,
    scopes: &[&str],
    extra_auth_params: &[(&str, &str)],
    userinfo_url: Option<&str>,
) -> Result<AuthResult, String> {
    let server = tiny_http::Server::http("127.0.0.1:0")
        .map_err(|e| format!("loopback bind: {}", e))?;
    let addr = server
        .server_addr()
        .to_ip()
        .ok_or_else(|| "no ip on loopback".to_string())?;
    let redirect = format!("http://127.0.0.1:{}", addr.port());

    let auth = AuthUrl::new(auth_url.into()).map_err(|e| format!("auth url: {}", e))?;
    let token = TokenUrl::new(token_url.into()).map_err(|e| format!("token url: {}", e))?;
    let client = BasicClient::new(
        ClientId::new(client_id.into()),
        client_secret.map(|s| ClientSecret::new(s.into())),
        auth,
        Some(token),
    )
    .set_redirect_uri(RedirectUrl::new(redirect.clone()).map_err(|e| e.to_string())?);

    let (challenge, verifier) = PkceCodeChallenge::new_random_sha256();
    let mut auth_req = client.authorize_url(CsrfToken::new_random);
    for s in scopes {
        auth_req = auth_req.add_scope(Scope::new((*s).into()));
    }
    for (k, v) in extra_auth_params {
        auth_req = auth_req.add_extra_param(*k, *v);
    }
    let (full_auth_url, csrf) = auth_req.set_pkce_challenge(challenge).url();

    open_in_browser(full_auth_url.as_str())?;

    // Algunos browsers re-piden /favicon.ico u otros paths post-redirect.
    // Aceptar requests hasta encontrar uno con `code` o `error`.
    let mut got_code: Option<String> = None;
    let mut got_state: Option<String> = None;
    let mut got_error: Option<String> = None;
    let deadline = std::time::Instant::now() + Duration::from_secs(300);

    while got_code.is_none() && got_error.is_none() {
        if std::time::Instant::now() > deadline {
            return Err("timeout esperando callback OAuth".into());
        }
        let req = server
            .recv_timeout(Duration::from_secs(5))
            .map_err(|e| format!("recv: {}", e))?;
        let Some(req) = req else { continue };

        let parsed = url::Url::parse(&format!("http://127.0.0.1{}", req.url()))
            .map_err(|e| format!("url parse: {}", e))?;
        for (k, v) in parsed.query_pairs() {
            match k.as_ref() {
                "code" => got_code = Some(v.to_string()),
                "state" => got_state = Some(v.to_string()),
                "error" => got_error = Some(v.to_string()),
                _ => {}
            }
        }

        let body = if got_error.is_some() {
            "<h1>Auth fall&oacute;</h1><p>Pod&eacute;s cerrar esta pesta&ntilde;a.</p>"
        } else if got_code.is_some() {
            "<h1>Auth OK</h1><p>Volv&eacute; a Kolibri. Pod&eacute;s cerrar esta pesta&ntilde;a.</p>"
        } else {
            "<h1>Esperando…</h1>"
        };
        let resp = tiny_http::Response::from_string(body)
            .with_header(
                tiny_http::Header::from_bytes(&b"Content-Type"[..], &b"text/html; charset=utf-8"[..])
                    .unwrap(),
            );
        let _ = req.respond(resp);
    }

    if let Some(e) = got_error {
        return Err(format!("oauth error: {}", e));
    }
    let code = got_code.ok_or_else(|| "no code".to_string())?;
    let state = got_state.unwrap_or_default();
    if state != *csrf.secret() {
        return Err("csrf state mismatch".into());
    }

    let token_response = client
        .exchange_code(AuthorizationCode::new(code))
        .set_pkce_verifier(verifier)
        .request_async(async_http_client)
        .await
        .map_err(|e| format!("token exchange: {}", e))?;

    let access = token_response.access_token().secret().clone();
    let refresh = token_response
        .refresh_token()
        .map(|t| t.secret().clone())
        .unwrap_or_default();
    let expires_in = token_response
        .expires_in()
        .map(|d| d.as_secs() as i64)
        .unwrap_or(3600);
    let scope_str = scopes.join(" ");

    let email = if let Some(url) = userinfo_url {
        fetch_email(url, &access).await.unwrap_or_default()
    } else {
        String::new()
    };

    Ok(AuthResult {
        tokens: TokenSet {
            access_token: access,
            refresh_token: refresh,
            expires_at: now_secs() + expires_in - 60, // margen de 60s
            scope: scope_str,
        },
        email,
    })
}

async fn fetch_email(url: &str, access_token: &str) -> Result<String, String> {
    let resp = reqwest::Client::new()
        .get(url)
        .bearer_auth(access_token)
        .send()
        .await
        .map_err(|e| e.to_string())?
        .json::<serde_json::Value>()
        .await
        .map_err(|e| e.to_string())?;
    Ok(resp
        .get("email")
        .or_else(|| resp.get("mail"))
        .or_else(|| resp.get("userPrincipalName"))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string())
}

fn open_in_browser(url: &str) -> Result<(), String> {
    // tauri-plugin-opener requiere AppHandle. Acá usamos comando del SO directo.
    #[cfg(target_os = "linux")]
    let cmd = ("xdg-open", [url]);
    #[cfg(target_os = "windows")]
    let cmd = ("cmd", ["/C", "start", "", url]);
    #[cfg(target_os = "macos")]
    let cmd = ("open", [url]);

    std::process::Command::new(cmd.0)
        .args(cmd.1)
        .spawn()
        .map_err(|e| format!("open browser: {}", e))?;
    Ok(())
}

pub async fn refresh_access(
    token_url: &str,
    client_id: &str,
    client_secret: Option<&str>,
    refresh_token: &str,
) -> Result<TokenSet, String> {
    let token = TokenUrl::new(token_url.into()).map_err(|e| e.to_string())?;
    // AuthUrl no se usa al refrescar pero BasicClient lo exige.
    let auth = AuthUrl::new("https://placeholder.invalid/auth".into())
        .map_err(|e| e.to_string())?;
    let client = BasicClient::new(
        ClientId::new(client_id.into()),
        client_secret.map(|s| ClientSecret::new(s.into())),
        auth,
        Some(token),
    );
    let resp = client
        .exchange_refresh_token(&RefreshToken::new(refresh_token.into()))
        .request_async(async_http_client)
        .await
        .map_err(|e| format!("refresh: {}", e))?;

    let access = resp.access_token().secret().clone();
    // Algunos providers (Google) NO devuelven nuevo refresh_token al refrescar:
    // mantener el viejo en ese caso.
    let new_refresh = resp
        .refresh_token()
        .map(|t| t.secret().clone())
        .unwrap_or_else(|| refresh_token.to_string());
    let expires_in = resp
        .expires_in()
        .map(|d| d.as_secs() as i64)
        .unwrap_or(3600);

    Ok(TokenSet {
        access_token: access,
        refresh_token: new_refresh,
        expires_at: now_secs() + expires_in - 60,
        scope: String::new(), // mantenemos scope original en config, no se renueva acá
    })
}

// ────────── Keyring storage ──────────

fn keyring_account(kind: &str, service_id: &str) -> String {
    format!("{}:{}", kind, service_id)
}

pub fn keyring_save(kind: &str, service_id: &str, tokens: &TokenSet) -> Result<(), String> {
    let entry = keyring::Entry::new(KEYRING_SERVICE, &keyring_account(kind, service_id))
        .map_err(|e| format!("keyring entry: {}", e))?;
    let json = serde_json::to_string(tokens).map_err(|e| e.to_string())?;
    entry.set_password(&json).map_err(|e| format!("keyring set: {}", e))
}

pub fn keyring_load(kind: &str, service_id: &str) -> Result<TokenSet, String> {
    let entry = keyring::Entry::new(KEYRING_SERVICE, &keyring_account(kind, service_id))
        .map_err(|e| format!("keyring entry: {}", e))?;
    let json = entry.get_password().map_err(|e| format!("keyring get: {}", e))?;
    serde_json::from_str(&json).map_err(|e| format!("keyring parse: {}", e))
}

pub fn keyring_delete(kind: &str, service_id: &str) -> Result<(), String> {
    let entry = keyring::Entry::new(KEYRING_SERVICE, &keyring_account(kind, service_id))
        .map_err(|e| format!("keyring entry: {}", e))?;
    entry
        .delete_credential()
        .map_err(|e| format!("keyring del: {}", e))
}

/// Devuelve un access_token válido. Si expiró, refresca y persiste.
pub async fn ensure_fresh_access(
    kind: &str,
    service_id: &str,
    token_url: &str,
    client_id: &str,
    client_secret: Option<&str>,
) -> Result<String, String> {
    let mut tokens = keyring_load(kind, service_id)?;
    if tokens.expires_at > now_secs() {
        return Ok(tokens.access_token);
    }
    if tokens.refresh_token.is_empty() {
        return Err("no refresh_token, reauth needed".into());
    }
    let new = refresh_access(token_url, client_id, client_secret, &tokens.refresh_token).await?;
    tokens.access_token = new.access_token;
    tokens.refresh_token = new.refresh_token;
    tokens.expires_at = new.expires_at;
    keyring_save(kind, service_id, &tokens)?;
    Ok(tokens.access_token)
}
