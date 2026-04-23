package dev.quartermaster.android

import android.Manifest
import android.net.Uri
import android.os.Bundle
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.remember
import androidx.lifecycle.lifecycleScope
import kotlinx.coroutines.launch

class MainActivity : ComponentActivity() {
    private var deepLinkHandler: ((Uri) -> Unit)? = null
    private var intentHandler: ((android.content.Intent) -> Unit)? = null

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        enableEdgeToEdge()

        setContent {
            val appState = remember { QuartermasterAppState.fromContext(applicationContext) }
            val permissionLauncher = rememberLauncherForActivityResult(
                contract = ActivityResultContracts.RequestPermission(),
            ) { granted ->
                lifecycleScope.launch {
                    appState.onNotificationPermissionResult(granted)
                }
            }
            deepLinkHandler = appState::handleDeepLink
            intentHandler = { nextIntent ->
                lifecycleScope.launch {
                    appState.handleIntent(nextIntent)
                }
            }
            LaunchedEffect(appState) {
                appState.bootstrap()
                appState.handleIntent(intent)
            }
            LaunchedEffect(appState.shouldRequestNotificationPermission) {
                if (appState.shouldRequestNotificationPermission) {
                    PushSupport.markNotificationPermissionPrompted(this@MainActivity)
                    permissionLauncher.launch(Manifest.permission.POST_NOTIFICATIONS)
                }
            }
            QuartermasterApp(appState = appState)
        }
    }

    override fun onNewIntent(intent: android.content.Intent) {
        super.onNewIntent(intent)
        setIntent(intent)
        intent.data?.let { deepLinkHandler?.invoke(it) }
        intentHandler?.invoke(intent)
    }
}
