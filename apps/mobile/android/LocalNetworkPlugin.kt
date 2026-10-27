package com.synapse.compass

import android.app.Activity
import android.os.Build
import app.tauri.annotation.Command
import app.tauri.annotation.Permission
import app.tauri.annotation.PermissionCallback
import app.tauri.annotation.TauriPlugin
import app.tauri.plugin.Invoke
import app.tauri.plugin.JSObject
import app.tauri.PermissionState
import app.tauri.plugin.Plugin

@TauriPlugin(
    permissions = [Permission(strings = ["android.permission.ACCESS_LOCAL_NETWORK"], alias = "localNetwork")]
)
class LocalNetworkPlugin(private val activity: Activity) : Plugin(activity) {
    @Command
    fun ensureAccess(invoke: Invoke) {
        // До Android 17 или при старом targetSdk отдельное разрешение запрашивать нельзя.
        if (Build.VERSION.SDK_INT < 37 || activity.applicationInfo.targetSdkVersion < 37) {
            respond(invoke, true)
            return
        }
        if (getPermissionState("localNetwork") == PermissionState.GRANTED) {
            respond(invoke, true)
        } else {
            requestPermissionForAlias("localNetwork", invoke, "localNetworkResult")
        }
    }

    @PermissionCallback
    private fun localNetworkResult(invoke: Invoke) {
        respond(invoke, getPermissionState("localNetwork") == PermissionState.GRANTED)
    }

    private fun respond(invoke: Invoke, granted: Boolean) {
        val result = JSObject()
        result.put("granted", granted)
        invoke.resolve(result)
    }
}
