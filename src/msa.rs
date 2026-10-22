use minecraft_msa_auth::{MinecraftAuthorizationFlow, UreqClient};
use oauth2::basic::BasicClient;
use oauth2::{
    AuthUrl, ClientId, DeviceAuthorizationUrl, Scope, StandardDeviceAuthorizationResponse,
    TokenResponse, TokenUrl,
};

const CLIENT_ID_ENV: &str = "NEUTRINO_MSA_CLIENT_ID";
const DEVICE_CODE_URL: &str = "https://login.microsoftonline.com/consumers/oauth2/v2.0/devicecode";
const AUTHORIZE_URL: &str = "https://login.microsoftonline.com/consumers/oauth2/v2.0/authorize";
const TOKEN_URL: &str = "https://login.microsoftonline.com/consumers/oauth2/v2.0/token";
const MINECRAFT_PROFILE_URL: &str = "https://api.minecraftservices.com/minecraft/profile";

pub enum LoginEvent {
    DeviceCode {
        verification_uri: String,
        user_code: String,
    },
    Success {
        player_name: String,
    },
    Error(String),
}

pub fn client_id() -> Result<String, String> {
    if let Some(client_id) = std::env::var(CLIENT_ID_ENV)
        .ok()
        .filter(|client_id| !client_id.trim().is_empty())
    {
        return Ok(client_id);
    }

    option_env!("NEUTRINO_MSA_CLIENT_ID")
        .filter(|client_id| !client_id.trim().is_empty())
        .map(str::to_string)
        .ok_or_else(|| format!("未设置 {CLIENT_ID_ENV} 环境变量"))
}

pub fn login<F>(client_id: String, on_event: F) -> Result<(), String>
where
    F: Fn(LoginEvent),
{
    let result = login_inner(client_id, &on_event);
    if let Err(error) = &result {
        on_event(LoginEvent::Error(error.clone()));
    }
    result
}

fn login_inner<F>(client_id: String, on_event: &F) -> Result<(), String>
where
    F: Fn(LoginEvent),
{
    let client = BasicClient::new(ClientId::new(client_id))
        .set_auth_uri(AuthUrl::new(AUTHORIZE_URL.to_string()).map_err(|error| error.to_string())?)
        .set_token_uri(TokenUrl::new(TOKEN_URL.to_string()).map_err(|error| error.to_string())?)
        .set_device_authorization_url(
            DeviceAuthorizationUrl::new(DEVICE_CODE_URL.to_string())
                .map_err(|error| error.to_string())?,
        );

    let http_client = oauth2::reqwest::blocking::ClientBuilder::new()
        .redirect(oauth2::reqwest::redirect::Policy::none())
        .build()
        .map_err(|error| error.to_string())?;

    let details: StandardDeviceAuthorizationResponse = client
        .exchange_device_code()
        .add_scope(Scope::new("XboxLive.signin offline_access".to_string()))
        .request(&http_client)
        .map_err(|error| error.to_string())?;

    let verification_uri = details.verification_uri().to_string();
    let user_code = details.user_code().secret().to_string();

    let _ = crate::open_url(&verification_uri);
    on_event(LoginEvent::DeviceCode {
        verification_uri,
        user_code,
    });

    let token = client
        .exchange_device_access_token(&details)
        .request(&http_client, std::thread::sleep, None)
        .map_err(|error| error.to_string())?;

    let minecraft = MinecraftAuthorizationFlow::new(UreqClient::default())
        .exchange_microsoft_token(token.access_token().secret())
        .map_err(|error| error.to_string())?;

    let player_name = fetch_profile_name(minecraft.access_token().as_ref())
        .unwrap_or_else(|| "Minecraft 账号".to_string());

    on_event(LoginEvent::Success { player_name });
    Ok(())
}

fn fetch_profile_name(access_token: &str) -> Option<String> {
    let profile: serde_json::Value = ureq::get(MINECRAFT_PROFILE_URL)
        .header("Authorization", &format!("Bearer {access_token}"))
        .call()
        .ok()?
        .body_mut()
        .read_json()
        .ok()?;

    profile
        .get("name")
        .and_then(|name| name.as_str())
        .map(str::to_string)
}
