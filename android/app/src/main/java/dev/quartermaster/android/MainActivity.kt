package dev.quartermaster.android

import android.net.Uri
import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.remember

class MainActivity : ComponentActivity() {
    private var deepLinkHandler: ((Uri) -> Unit)? = null

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        enableEdgeToEdge()

        setContent {
            val appState = remember { QuartermasterAppState.fromContext(applicationContext) }
            deepLinkHandler = appState::handleDeepLink
            LaunchedEffect(appState) {
                appState.bootstrap()
                intent?.data?.let(appState::handleDeepLink)
            }
            QuartermasterApp(appState = appState)
        }
    }

    override fun onNewIntent(intent: android.content.Intent) {
        super.onNewIntent(intent)
        setIntent(intent)
        intent.data?.let { deepLinkHandler?.invoke(it) }
    }
}
