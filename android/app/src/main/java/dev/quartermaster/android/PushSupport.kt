package dev.quartermaster.android

import android.Manifest
import android.app.Application
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.content.Context
import android.content.Intent
import android.content.SharedPreferences
import android.os.Build
import androidx.core.app.NotificationCompat
import androidx.core.app.NotificationManagerCompat
import androidx.core.content.ContextCompat
import com.google.android.gms.tasks.Task
import com.google.firebase.FirebaseApp
import com.google.firebase.FirebaseOptions
import com.google.firebase.messaging.FirebaseMessaging
import com.google.firebase.messaging.FirebaseMessagingService
import com.google.firebase.messaging.RemoteMessage
import dev.quartermaster.android.generated.models.PushAuthorizationStatus
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.launch
import kotlinx.coroutines.suspendCancellableCoroutine
import kotlin.coroutines.resume
import kotlin.coroutines.resumeWithException

data class ReminderPushPayload(
    val reminderId: String,
    val batchId: String,
    val productId: String,
    val locationId: String,
    val kind: String,
    val title: String,
    val body: String,
)

class QuartermasterApplication : Application() {
    override fun onCreate() {
        super.onCreate()
        PushSupport.initialize(this)
        PushSupport.ensureNotificationChannel(this)
    }
}

class QuartermasterFirebaseMessagingService : FirebaseMessagingService() {
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.IO)

    override fun onMessageReceived(message: RemoteMessage) {
        val payload = PushSupport.payloadFromMap(message.data) ?: return
        PushSupport.postReminderNotification(this, payload)
        scope.launch {
            PushSupport.presentReminder(this@QuartermasterFirebaseMessagingService, payload.reminderId)
        }
    }

    override fun onNewToken(token: String) {
        scope.launch {
            PushSupport.syncDeviceRegistration(
                context = this@QuartermasterFirebaseMessagingService,
                pushTokenOverride = token,
            )
        }
    }
}

object PushSupport {
    private const val PUSH_PREFS = "quartermaster-push"
    private const val KEY_PERMISSION_PROMPTED = "notification_permission_prompted"

    const val CHANNEL_ID = "expiry_reminders"
    private const val EXTRA_REMINDER_ID = "quartermaster.reminder_id"
    private const val EXTRA_BATCH_ID = "quartermaster.batch_id"
    private const val EXTRA_PRODUCT_ID = "quartermaster.product_id"
    private const val EXTRA_LOCATION_ID = "quartermaster.location_id"
    private const val EXTRA_KIND = "quartermaster.kind"
    private const val EXTRA_TITLE = "quartermaster.title"
    private const val EXTRA_BODY = "quartermaster.body"

    fun isFirebaseConfigured(): Boolean = BuildConfig.FIREBASE_PROJECT_ID.isNotBlank() &&
        BuildConfig.FIREBASE_APPLICATION_ID.isNotBlank() &&
        BuildConfig.FIREBASE_API_KEY.isNotBlank() &&
        BuildConfig.FIREBASE_SENDER_ID.isNotBlank()

    fun initialize(context: Context): Boolean {
        if (!isFirebaseConfigured()) return false
        if (FirebaseApp.getApps(context).isNotEmpty()) return true
        val options =
            FirebaseOptions
                .Builder()
                .setProjectId(BuildConfig.FIREBASE_PROJECT_ID)
                .setApplicationId(BuildConfig.FIREBASE_APPLICATION_ID)
                .setApiKey(BuildConfig.FIREBASE_API_KEY)
                .setGcmSenderId(BuildConfig.FIREBASE_SENDER_ID)
                .build()
        FirebaseApp.initializeApp(context, options)
        return true
    }

    fun ensureNotificationChannel(context: Context) {
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.O) return
        val manager = context.getSystemService(NotificationManager::class.java) ?: return
        val channel =
            NotificationChannel(
                CHANNEL_ID,
                "Expiry reminders",
                NotificationManager.IMPORTANCE_HIGH,
            ).apply {
                description = "Quartermaster expiry and stock reminders"
            }
        manager.createNotificationChannel(channel)
    }

    fun currentAuthorization(context: Context): PushAuthorizationStatus {
        if (!isFirebaseConfigured()) return PushAuthorizationStatus.DENIED
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
            val granted =
                ContextCompat.checkSelfPermission(
                    context,
                    Manifest.permission.POST_NOTIFICATIONS,
                ) == android.content.pm.PackageManager.PERMISSION_GRANTED
            if (granted) return PushAuthorizationStatus.AUTHORIZED
            return if (prefs(context).getBoolean(KEY_PERMISSION_PROMPTED, false)) {
                PushAuthorizationStatus.DENIED
            } else {
                PushAuthorizationStatus.NOT_DETERMINED
            }
        }
        return if (NotificationManagerCompat.from(context).areNotificationsEnabled()) {
            PushAuthorizationStatus.AUTHORIZED
        } else {
            PushAuthorizationStatus.DENIED
        }
    }

    fun markNotificationPermissionPrompted(context: Context) {
        prefs(context).edit().putBoolean(KEY_PERMISSION_PROMPTED, true).apply()
    }

    suspend fun syncDeviceRegistration(
        context: Context,
        backend: QuartermasterBackend? = null,
        deviceId: String? = null,
        authorizationOverride: PushAuthorizationStatus? = null,
        pushTokenOverride: String? = null,
    ) {
        val appContext = context.applicationContext
        val authStore = AuthStore(appContext)
        if (authStore.snapshot().accessToken.isNullOrBlank()) return
        val resolvedBackend = backend ?: QuartermasterApiBackend(QuartermasterApi(authStore))
        val resolvedDeviceId = deviceId ?: authStore.stableDeviceId()
        val authorization = authorizationOverride ?: currentAuthorization(appContext)
        val token =
            if (authorization == PushAuthorizationStatus.AUTHORIZED ||
                authorization == PushAuthorizationStatus.PROVISIONAL
            ) {
                pushTokenOverride ?: currentToken(appContext)
            } else {
                null
            }
        runCatching {
            resolvedBackend.registerDevice(
                deviceId = resolvedDeviceId,
                pushToken = token,
                authorization = authorization,
                appVersion = BuildConfig.VERSION_NAME,
            )
        }
    }

    suspend fun clearDeviceRegistration(
        context: Context,
        backend: QuartermasterBackend? = null,
        deviceId: String? = null,
    ) {
        syncDeviceRegistration(
            context = context,
            backend = backend,
            deviceId = deviceId,
            authorizationOverride = PushAuthorizationStatus.DENIED,
            pushTokenOverride = null,
        )
    }

    suspend fun presentReminder(
        context: Context,
        reminderId: String,
    ) {
        val authStore = AuthStore(context.applicationContext)
        if (authStore.snapshot().accessToken.isNullOrBlank()) return
        runCatching {
            QuartermasterApiBackend(QuartermasterApi(authStore)).presentReminder(reminderId)
        }
    }

    fun payloadFromIntent(intent: Intent?): ReminderPushPayload? {
        val extras = intent?.extras ?: return null
        val reminderId = extras.getString(EXTRA_REMINDER_ID) ?: return null
        val batchId = extras.getString(EXTRA_BATCH_ID) ?: return null
        val productId = extras.getString(EXTRA_PRODUCT_ID) ?: return null
        val locationId = extras.getString(EXTRA_LOCATION_ID) ?: return null
        val kind = extras.getString(EXTRA_KIND) ?: return null
        val title = extras.getString(EXTRA_TITLE) ?: return null
        val body = extras.getString(EXTRA_BODY) ?: return null
        return ReminderPushPayload(
            reminderId = reminderId,
            batchId = batchId,
            productId = productId,
            locationId = locationId,
            kind = kind,
            title = title,
            body = body,
        )
    }

    fun applyReminderPayload(
        intent: Intent,
        payload: ReminderPushPayload,
    ): Intent = intent
        .putExtra(EXTRA_REMINDER_ID, payload.reminderId)
        .putExtra(EXTRA_BATCH_ID, payload.batchId)
        .putExtra(EXTRA_PRODUCT_ID, payload.productId)
        .putExtra(EXTRA_LOCATION_ID, payload.locationId)
        .putExtra(EXTRA_KIND, payload.kind)
        .putExtra(EXTRA_TITLE, payload.title)
        .putExtra(EXTRA_BODY, payload.body)

    fun payloadFromMap(data: Map<String, String>): ReminderPushPayload? {
        val reminderId = data["reminder_id"] ?: return null
        val batchId = data["batch_id"] ?: return null
        val productId = data["product_id"] ?: return null
        val locationId = data["location_id"] ?: return null
        val kind = data["kind"] ?: return null
        val title = data["title"] ?: return null
        val body = data["body"] ?: return null
        return ReminderPushPayload(
            reminderId = reminderId,
            batchId = batchId,
            productId = productId,
            locationId = locationId,
            kind = kind,
            title = title,
            body = body,
        )
    }

    fun postReminderNotification(
        context: Context,
        payload: ReminderPushPayload,
    ) {
        ensureNotificationChannel(context)
        val intent =
            applyReminderPayload(
                Intent(context, MainActivity::class.java)
                    .addFlags(Intent.FLAG_ACTIVITY_CLEAR_TOP or Intent.FLAG_ACTIVITY_SINGLE_TOP),
                payload,
            )
        val pendingIntent =
            PendingIntent.getActivity(
                context,
                payload.reminderId.hashCode(),
                intent,
                PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE,
            )
        val notification =
            NotificationCompat
                .Builder(context, CHANNEL_ID)
                .setSmallIcon(android.R.drawable.ic_dialog_info)
                .setContentTitle(payload.title)
                .setContentText(payload.body)
                .setPriority(NotificationCompat.PRIORITY_HIGH)
                .setAutoCancel(true)
                .setContentIntent(pendingIntent)
                .build()
        NotificationManagerCompat.from(context).notify(payload.reminderId.hashCode(), notification)
    }

    private suspend fun currentToken(context: Context): String? {
        if (!initialize(context)) return null
        return runCatching { FirebaseMessaging.getInstance().token.await() }.getOrNull()
    }

    private fun prefs(context: Context): SharedPreferences = context.getSharedPreferences(PUSH_PREFS, Context.MODE_PRIVATE)
}

private suspend fun <T> Task<T>.await(): T = suspendCancellableCoroutine { continuation ->
    addOnSuccessListener { continuation.resume(it) }
    addOnFailureListener { continuation.resumeWithException(it) }
}
