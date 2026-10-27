use serde::Serialize;
use serde_json::Value;
use std::time::Duration;
mod local_network;

struct ApiClient {
    client: reqwest::Client,
    base: String,
    needs_local_network: bool,
}

#[derive(Serialize)]
struct ApiFailure {
    code: &'static str,
    message: &'static str,
}

impl ApiFailure {
    fn new(code: &'static str, message: &'static str) -> Self {
        Self { code, message }
    }
}

#[derive(Serialize)]
struct ApiResponse {
    status: u16,
    body: Value,
}

#[tauri::command]
async fn api_request(
    app: tauri::AppHandle,
    state: tauri::State<'_, ApiClient>,
    path: String,
    method: String,
    body: Value,
    query: Option<std::collections::HashMap<String, String>>,
) -> Result<ApiResponse, ApiFailure> {
    // Через команду доступен только настроенный сервер API; произвольные адреса и заголовки запрещены.
    if !path.starts_with('/')
        || path.contains("..")
        || path.contains(['?', '#', '\\', ':', '%'])
        || path.starts_with("//")
    {
        return Err(ApiFailure::new(
            "invalid_path",
            "Некорректный путь запроса приложения",
        ));
    }
    let method = match method.as_str() {
        "GET" => reqwest::Method::GET,
        "POST" => reqwest::Method::POST,
        "PATCH" => reqwest::Method::PATCH,
        _ => {
            return Err(ApiFailure::new(
                "invalid_method",
                "Неподдерживаемый метод запроса приложения",
            ));
        }
    };
    local_network::ensure(&app, state.needs_local_network).await?;
    let mut request = state
        .client
        .request(method, format!("{}/api/v1{}", state.base, path))
        .header("X-Synapse-Request", "1");
    if let Some(query) = query {
        if query.len() > 16
            || query
                .iter()
                .any(|(key, value)| key.len() > 64 || value.len() > 512)
        {
            return Err(ApiFailure::new(
                "invalid_query",
                "Некорректные параметры запроса",
            ));
        }
        request = request.query(&query);
    }
    if !body.is_null() {
        request = request.json(&body);
    }
    let response = request.send().await.map_err(|error| {
        if error.is_timeout() {
            ApiFailure::new(
                "network_timeout",
                "Сервер не ответил вовремя. Проверьте его доступность и разрешение локальной сети.",
            )
        } else {
            ApiFailure::new(
                "network_unavailable",
                "Не удалось подключиться к серверу. Проверьте подключение и работу сервера.",
            )
        }
    })?;
    let status = response.status().as_u16();
    let bytes = response.bytes().await.map_err(|_| {
        ApiFailure::new(
            "response_failed",
            "Соединение прервалось при получении ответа сервера",
        )
    })?;
    let body = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes).unwrap_or(Value::Null)
    };
    Ok(ApiResponse { status, body })
}

#[tauri::command]
fn connection_info(state: tauri::State<'_, ApiClient>) -> Value {
    serde_json::json!({"api_url":state.base,"local_network_required":state.needs_local_network})
}

fn local_api(base: &str) -> bool {
    let Ok(url) = reqwest::Url::parse(base) else {
        return false;
    };
    let host = url.host_str().unwrap_or_default().trim_matches(['[', ']']);
    match host.parse::<std::net::IpAddr>() {
        Ok(std::net::IpAddr::V4(ip)) => ip.is_private() || ip.is_loopback() || ip.is_link_local(),
        Ok(std::net::IpAddr::V6(ip)) => {
            ip.is_unique_local() || ip.is_loopback() || ip.is_unicast_link_local()
        }
        Err(_) => host == "localhost" || host.ends_with(".localhost") || host.ends_with(".local"),
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Адрес задаётся при сборке. Эмулятор Android обращается к компьютеру через 10.0.2.2.
    let default_api = if cfg!(all(target_os = "android", debug_assertions)) {
        "http://10.0.2.2:3000"
    } else {
        "http://127.0.0.1:3000"
    };
    let base = option_env!("SYNAPSE_API_URL")
        .unwrap_or(default_api)
        .trim_end_matches('/')
        .to_string();
    let client = reqwest::Client::builder()
        .cookie_store(true)
        .timeout(Duration::from_secs(15))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .expect("HTTP client configuration");
    let needs_local_network = local_api(&base);
    let builder = tauri::Builder::default();
    #[cfg(target_os = "android")]
    let builder = builder.plugin(local_network::init());
    builder
        .manage(ApiClient {
            client,
            base,
            needs_local_network,
        })
        .invoke_handler(tauri::generate_handler![api_request, connection_info])
        .run(tauri::generate_context!())
        .expect("Tauri application failed");
}

#[cfg(test)]
mod tests {
    use super::local_api;

    #[test]
    fn requests_local_permission_only_for_local_hosts() {
        for host in [
            "http://10.0.2.2:3000",
            "http://192.168.1.10",
            "http://localhost:3000",
            "http://[::1]:3000",
            "http://[fd00::1]",
            "https://server.local",
        ] {
            assert!(local_api(host), "{host}");
        }
        for host in [
            "https://example.com",
            "https://8.8.8.8",
            "https://localhost.example.com",
            "не адрес",
        ] {
            assert!(!local_api(host), "{host}");
        }
    }
}
