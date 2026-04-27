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
    val productName: String,
    val locationName: String,
    val quantity: String,
    val unit: String,
    val expiresOn: String?,
)

fun reminderNotificationTitle(payload: ReminderPushPayload): String = "${payload.productName} in ${payload.locationName}"

fun reminderNotificationBody(payload: ReminderPushPayload): String = if (payload.expiresOn == null) {
    "${payload.quantity} ${payload.unit} has an expiry reminder."
} else {
    "${payload.quantity} ${payload.unit} expires on ${payload.expiresOn}."
}

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
    private const val EXTRA_PRODUCT_NAME = "quartermaster.product_name"
    private const val EXTRA_LOCATION_NAME = "quartermaster.location_name"
    private const val EXTRA_QUANTITY = "quartermaster.quantity"
    private const val EXTRA_UNIT = "quartermaster.unit"
    private const val EXTRA_EXPIRES_ON = "quartermaster.expires_on"

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
        val productName = extras.getString(EXTRA_PRODUCT_NAME) ?: return null
        val locationName = extras.getString(EXTRA_LOCATION_NAME) ?: return null
        val quantity = extras.getString(EXTRA_QUANTITY) ?: return null
        val unit = extras.getString(EXTRA_UNIT) ?: return null
        return ReminderPushPayload(
            reminderId = reminderId,
            batchId = batchId,
            productId = productId,
            locationId = locationId,
            kind = kind,
            productName = productName,
            locationName = locationName,
            quantity = quantity,
            unit = unit,
            expiresOn = extras.getString(EXTRA_EXPIRES_ON),
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
        .putExtra(EXTRA_PRODUCT_NAME, payload.productName)
        .putExtra(EXTRA_LOCATION_NAME, payload.locationName)
        .putExtra(EXTRA_QUANTITY, payload.quantity)
        .putExtra(EXTRA_UNIT, payload.unit)
        .putExtra(EXTRA_EXPIRES_ON, payload.expiresOn)

    fun payloadFromMap(data: Map<String, String>): ReminderPushPayload? {
        val reminderId = data["reminder_id"] ?: return null
        val batchId = data["batch_id"] ?: return null
        val productId = data["product_id"] ?: return null
        val locationId = data["location_id"] ?: return null
        val kind = data["kind"] ?: return null
        val productName = data["product_name"] ?: return null
        val locationName = data["location_name"] ?: return null
        val quantity = data["quantity"] ?: return null
        val unit = data["unit"] ?: return null
        return ReminderPushPayload(
            reminderId = reminderId,
            batchId = batchId,
            productId = productId,
            locationId = locationId,
            kind = kind,
            productName = productName,
            locationName = locationName,
            quantity = quantity,
            unit = unit,
            expiresOn = data["expires_on"]?.takeIf(String::isNotBlank),
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
                .setContentTitle(reminderNotificationTitle(payload))
                .setContentText(reminderNotificationBody(payload))
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
