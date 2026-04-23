package dev.quartermaster.android

import android.content.Context
import android.content.SharedPreferences
import androidx.security.crypto.EncryptedSharedPreferences
import androidx.security.crypto.MasterKey

data class SessionSnapshot(
    val serverUrl: String,
    val accessToken: String?,
    val refreshToken: String?,
)

class AuthStore(context: Context) : SessionStore {
    private val prefs: SharedPreferences

    init {
        val masterKey = MasterKey.Builder(context)
            .setKeyScheme(MasterKey.KeyScheme.AES256_GCM)
            .build()
        prefs = EncryptedSharedPreferences.create(
            context,
            "quartermaster-session",
            masterKey,
            EncryptedSharedPreferences.PrefKeyEncryptionScheme.AES256_SIV,
            EncryptedSharedPreferences.PrefValueEncryptionScheme.AES256_GCM,
        )
    }

    override fun snapshot(): SessionSnapshot = SessionSnapshot(
        serverUrl = prefs.getString(KEY_SERVER_URL, DEFAULT_SERVER_URL) ?: DEFAULT_SERVER_URL,
        accessToken = prefs.getString(KEY_ACCESS_TOKEN, null),
        refreshToken = prefs.getString(KEY_REFRESH_TOKEN, null),
    )

    override fun saveServerUrl(url: String) {
        prefs.edit().putString(KEY_SERVER_URL, url.trim().removeSuffix("/")).apply()
    }

    override fun saveTokens(accessToken: String, refreshToken: String) {
        prefs.edit()
            .putString(KEY_ACCESS_TOKEN, accessToken)
            .putString(KEY_REFRESH_TOKEN, refreshToken)
            .apply()
    }

    override fun clearTokens() {
        prefs.edit()
            .remove(KEY_ACCESS_TOKEN)
            .remove(KEY_REFRESH_TOKEN)
            .apply()
    }

    override fun stableDeviceId(): String {
        val existing = prefs.getString(KEY_DEVICE_ID, null)
        if (!existing.isNullOrBlank()) return existing
        val created = java.util.UUID.randomUUID().toString().lowercase()
        prefs.edit().putString(KEY_DEVICE_ID, created).apply()
        return created
    }

    companion object {
        const val DEFAULT_SERVER_URL = "http://10.0.2.2:8080"

        private const val KEY_SERVER_URL = "server_url"
        private const val KEY_ACCESS_TOKEN = "access_token"
        private const val KEY_REFRESH_TOKEN = "refresh_token"
        private const val KEY_DEVICE_ID = "device_id"
    }
}
