use crate::ApiFailure;

#[cfg(target_os = "android")]
use tauri::{
    Manager, Runtime,
    plugin::{PluginHandle, TauriPlugin},
};

#[cfg(target_os = "android")]
struct LocalNetwork<R: Runtime>(PluginHandle<R>);

#[cfg(target_os = "android")]
pub fn init<R: Runtime>() -> TauriPlugin<R> {
    tauri::plugin::Builder::new("synapse-local-network")
        .setup(|app, api| {
            let handle =
                api.register_android_plugin("com.synapse.compass", "LocalNetworkPlugin")?;
            app.manage(LocalNetwork(handle));
            Ok(())
        })
        .build()
}

pub async fn ensure(app: &tauri::AppHandle, required: bool) -> Result<(), ApiFailure> {
    #[cfg(target_os = "android")]
    if required {
        #[derive(serde::Deserialize)]
        struct PermissionResponse {
            granted: bool,
        }
        let result: PermissionResponse = app
            .state::<LocalNetwork<tauri::Wry>>()
            .0
            .run_mobile_plugin_async("ensureAccess", ())
            .await
            .map_err(|_| {
                ApiFailure::new(
                    "permission_check_failed",
                    "Не удалось проверить разрешение локальной сети. Перезапустите приложение.",
                )
            })?;
        if !result.granted {
            return Err(ApiFailure::new(
                "local_network_denied",
                "Для подключения к локальному серверу разрешите доступ к устройствам поблизости в настройках приложения.",
            ));
        }
    }
    let _ = (app, required);
    Ok(())
}
