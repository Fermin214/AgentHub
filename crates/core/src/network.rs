//! Application-scoped networking; never changes system or global Git settings.
use crate::store::Store;
use anyhow::{bail, Context, Result};
use rusqlite::OptionalExtension;
use serde_json::{json, Value};
pub fn validate_proxy(value: &str) -> Result<String> {
    let value = value.trim();
    if value.is_empty() {
        return Ok(String::new());
    }
    let url = reqwest::Url::parse(value).context("Proxy must be an HTTP or HTTPS URL")?;
    if !matches!(url.scheme(), "http" | "https")
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
        || url.path() != "/"
    {
        bail!("Proxy must use HTTP or HTTPS with a host, without credentials, path, query, or fragment");
    }
    Ok(url.as_str().trim_end_matches('/').to_owned())
}
pub fn proxy_url(store: &Store) -> Result<String> {
    let saved: Option<String> = store
        .conn
        .query_row(
            "SELECT value_json FROM app_settings WHERE key = 'network_v1'",
            [],
            |r| r.get(0),
        )
        .optional()?;
    match saved {
        None => Ok(String::new()),
        Some(saved) => {
            let value: Value = serde_json::from_str(&saved)?;
            validate_proxy(
                value["proxyUrl"]
                    .as_str()
                    .context("Saved network proxy is invalid")?,
            )
        }
    }
}
pub fn dispatch(store: &Store, method: &str, args: &Value) -> Result<Value> {
    match method {
        "network.get" => Ok(json!({"proxyUrl": proxy_url(store)?})),
        "network.save" => {
            let proxy = validate_proxy(
                args["proxyUrl"]
                    .as_str()
                    .context("proxyUrl must be a string")?,
            )?;
            let value = json!({"proxyUrl": proxy});
            store.conn.execute(
                "INSERT INTO app_settings(key,value_json) VALUES ('network_v1',?1) ON CONFLICT(key) DO UPDATE SET value_json=excluded.value_json",
                [serde_json::to_string(&value)?]
            )?;
            Ok(value)
        }
        _ => bail!("Unknown network method"),
    }
}
pub fn client_builder(proxy: Option<&str>) -> Result<reqwest::blocking::ClientBuilder> {
    let mut builder = reqwest::blocking::Client::builder()
        .user_agent(concat!("AgentHub/", env!("CARGO_PKG_VERSION")))
        .https_only(true);
    let proxy = validate_proxy(proxy.unwrap_or(""))?;
    if !proxy.is_empty() {
        builder = builder.proxy(reqwest::Proxy::all(&proxy)?);
    }
    Ok(builder)
}
