package dev.quartermaster.android

import android.Manifest
import android.net.Uri
import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.remember
import androidx.lifecycle.lifecycleScope
import kotlinx.coroutines.launch

internal class ReminderIntentRouter(
    private val handleDeepLink: suspend (Uri) -> Unit,
    private val handleIntent: suspend (android.content.Intent) -> Unit,
) {
    private var lastHandledSignature: String? = null

    suspend fun route(intent: android.content.Intent?) {
        intent ?: return
        val signature = intentSignature(intent)
        if (signature != null && signature == lastHandledSignature) {
            return
        }
        signature?.let { lastHandledSignature = it }
        intent.data?.let { handleDeepLink(it) }
        handleIntent(intent)
    }

    internal fun intentSignature(intent: android.content.Intent): String? {
        val deepLink = intent.dataString
        val payload = PushSupport.payloadFromIntent(intent)
        if (deepLink == null && payload == null) {
            return null
        }
        return buildString {
            append(deepLink ?: "")
            append("|")
            append(payload?.reminderId ?: "")
            append("|")
            append(payload?.batchId ?: "")
            append("|")
            append(payload?.productId ?: "")
            append("|")
            append(payload?.locationId ?: "")
        }
    }
}

class MainActivity : ComponentActivity() {
    private var intentRouter: ReminderIntentRouter? = null

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        enableEdgeToEdge()

        setContent {
            val appState = remember { QuartermasterAppState.fromContext(applicationContext) }
            val permissionLauncher =
                rememberLauncherForActivityResult(
                    contract = ActivityResultContracts.RequestPermission(),
                ) { granted ->
                    lifecycleScope.launch {
                        appState.onNotificationPermissionResult(granted)
                    }
                }
            intentRouter =
                ReminderIntentRouter(
                    handleDeepLink = appState::handleIncomingDeepLink,
                    handleIntent = appState::handleIntent,
                )
            LaunchedEffect(appState) {
                appState.bootstrap()
                intentRouter?.route(intent)
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
        lifecycleScope.launch {
            intentRouter?.route(intent)
        }
    }
}
