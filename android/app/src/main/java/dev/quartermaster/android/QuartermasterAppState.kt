package dev.quartermaster.android

import android.content.Context
import android.net.Uri
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import dev.quartermaster.android.generated.models.BarcodeLookupResponse
import dev.quartermaster.android.generated.models.CreateInviteRequest
import dev.quartermaster.android.generated.models.CreateStockRequest
import dev.quartermaster.android.generated.models.HouseholdDetailDto
import dev.quartermaster.android.generated.models.InviteDto
import dev.quartermaster.android.generated.models.LocationDto
import dev.quartermaster.android.generated.models.MembershipRole
import dev.quartermaster.android.generated.models.MeResponse
import dev.quartermaster.android.generated.models.ProductDto
import dev.quartermaster.android.generated.models.PushAuthorizationStatus
import dev.quartermaster.android.generated.models.ReminderDto
import dev.quartermaster.android.generated.models.StockBatchDto
import dev.quartermaster.android.generated.models.StockEventDto
import dev.quartermaster.android.generated.models.UnitDto
import java.util.UUID
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext

enum class MainTab { Inventory, Reminders, Scan, Settings }

sealed interface AppPhase {
    data object Launching : AppPhase
    data class LaunchFailed(val message: String) : AppPhase
    data object Unauthenticated : AppPhase
    data class Authenticated(val me: MeResponse) : AppPhase
}

class QuartermasterAppState(context: Context) {
    private val authStore = AuthStore(context)
    private val api = QuartermasterApi(authStore)

    var phase: AppPhase by mutableStateOf(AppPhase.Launching)
        private set
    var selectedTab by mutableStateOf(MainTab.Inventory)
    var lastError by mutableStateOf<String?>(null)
    var serverUrl by mutableStateOf(authStore.snapshot().serverUrl)
        private set

    var units by mutableStateOf<List<UnitDto>>(emptyList())
        private set
    var locations by mutableStateOf<List<LocationDto>>(emptyList())
        private set
    var batches by mutableStateOf<List<StockBatchDto>>(emptyList())
        private set
    var reminders by mutableStateOf<List<ReminderDto>>(emptyList())
        private set
    var history by mutableStateOf<List<StockEventDto>>(emptyList())
        private set
    var searchResults by mutableStateOf<List<ProductDto>>(emptyList())
        private set
    var selectedProduct by mutableStateOf<ProductDto?>(null)
        private set
    var householdDetail by mutableStateOf<HouseholdDetailDto?>(null)
        private set
    var invites by mutableStateOf<List<InviteDto>>(emptyList())
        private set

    var isBusy by mutableStateOf(false)
        private set
    var isLoadingInventory by mutableStateOf(false)
        private set
    var isLoadingReminders by mutableStateOf(false)
        private set
    var pendingInviteCode by mutableStateOf<String?>(null)
        private set

    suspend fun bootstrap() {
        val snapshot = authStore.snapshot()
        serverUrl = snapshot.serverUrl
        if (snapshot.accessToken.isNullOrBlank()) {
            phase = AppPhase.Unauthenticated
            return
        }
        runCatching { api.me() }
            .onSuccess { applyAuthenticated(it) }
            .onFailure {
                authStore.clearTokens()
                phase = AppPhase.LaunchFailed(it.userFacingMessage())
            }
    }

    fun handleDeepLink(uri: Uri) {
        val invite = uri.getQueryParameter("invite")?.takeIf { it.isNotBlank() }
        val server = uri.getQueryParameter("server")?.takeIf { it.startsWith("http") }
        if (server != null) updateServerUrl(server)
        pendingInviteCode = invite
        selectedTab = MainTab.Settings
    }

    fun updateServerUrl(url: String) {
        serverUrl = url.trim().removeSuffix("/")
        api.serverUrl = serverUrl
    }

    suspend fun signIn(username: String, password: String) = busy {
        api.login(username = username, password = password)
        applyAuthenticated(api.me())
    }

    suspend fun register(
        username: String,
        password: String,
        email: String?,
        inviteCode: String?,
    ) = busy {
        api.register(username = username, password = password, email = email, inviteCode = inviteCode)
        applyAuthenticated(api.me())
    }

    suspend fun logout() = busy {
        api.logout()
        clearSession()
        phase = AppPhase.Unauthenticated
    }

    suspend fun switchHousehold(householdId: String) = busy {
        applyAuthenticated(api.switchHousehold(householdId))
    }

    suspend fun createHousehold(name: String, timezone: String) = busy {
        applyAuthenticated(api.createHousehold(name = name, timezone = timezone))
    }

    suspend fun redeemInvite(code: String) = busy {
        api.redeemInvite(code)
        applyAuthenticated(api.me())
    }

    suspend fun createInvite(expiresAt: String, maxUses: Long) = busy {
        val invite = api.createInvite(
            CreateInviteRequest(
                expiresAt = expiresAt,
                maxUses = maxUses,
                roleGranted = MembershipRole.MEMBER,
            )
        )
        invites = listOf(invite) + invites
    }

    suspend fun refreshInventory() = guardHousehold {
        isLoadingInventory = true
        try {
            if (units.isEmpty()) units = api.units()
            locations = api.locations().sortedBy { it.sortOrder ?: 0L }
            batches = api.listStock()
            history = api.listEvents()
        } finally {
            isLoadingInventory = false
        }
    }

    suspend fun refreshReminders() = guardHousehold {
        isLoadingReminders = true
        try {
            reminders = api.listReminders()
            reminders.filter { it.presentedOnDeviceAt == null }.forEach { reminder ->
                runCatching { api.presentReminder(reminder.id) }
            }
        } finally {
            isLoadingReminders = false
        }
    }

    suspend fun loadSettings() = guardHousehold {
        householdDetail = api.currentHousehold()
        invites = runCatching { api.householdInvites() }.getOrDefault(emptyList())
    }

    suspend fun searchProducts(query: String) = busy {
        searchResults = if (query.isBlank()) emptyList() else api.searchProducts(query)
    }

    suspend fun lookupBarcode(barcode: String) = busy {
        val response: BarcodeLookupResponse = api.lookupBarcode(barcode)
        selectedProduct = response.product
    }

    fun selectProduct(product: ProductDto) {
        selectedProduct = product
    }

    suspend fun addStock(
        productId: String,
        locationId: String,
        quantity: String,
        unit: String,
        expiresOn: String?,
        note: String?,
    ) = busy {
        api.addStock(
            CreateStockRequest(
                productId = productId,
                locationId = locationId,
                quantity = quantity,
                unit = unit,
                expiresOn = expiresOn,
                openedOn = null,
                note = note,
            )
        )
        refreshInventory()
        refreshReminders()
    }

    suspend fun acknowledgeReminder(id: String) = busy {
        api.acknowledgeReminder(id)
        reminders = reminders.filterNot { it.id == id }
    }

    suspend fun openReminder(reminder: ReminderDto) = busy {
        api.openReminder(reminder.id)
        selectedTab = MainTab.Inventory
    }

    suspend fun registerDevice() {
        runCatching {
            api.registerDevice(
                deviceId = UUID.nameUUIDFromBytes("quartermaster-android".toByteArray()).toString(),
                pushToken = null,
                authorization = PushAuthorizationStatus.DENIED,
                appVersion = "0.1.0",
            )
        }
    }

    val meOrNull: MeResponse?
        get() = (phase as? AppPhase.Authenticated)?.me

    val currentHouseholdId: String?
        get() = meOrNull?.currentHousehold?.id

    private suspend fun applyAuthenticated(me: MeResponse) {
        phase = AppPhase.Authenticated(me)
        lastError = null
        pendingInviteCode = null
        registerDevice()
        if (me.currentHousehold != null) {
            refreshInventory()
            refreshReminders()
            loadSettings()
        } else {
            locations = emptyList()
            batches = emptyList()
            reminders = emptyList()
            history = emptyList()
            householdDetail = null
        }
    }

    private fun clearSession() {
        units = emptyList()
        locations = emptyList()
        batches = emptyList()
        reminders = emptyList()
        history = emptyList()
        searchResults = emptyList()
        selectedProduct = null
        householdDetail = null
        invites = emptyList()
        lastError = null
    }

    private suspend fun busy(block: suspend () -> Unit) {
        isBusy = true
        lastError = null
        try {
            block()
        } catch (failure: Throwable) {
            if (failure is ApiFailure && failure.status == 401) {
                clearSession()
                phase = AppPhase.Unauthenticated
            } else {
                lastError = failure.userFacingMessage()
            }
        } finally {
            isBusy = false
        }
    }

    private suspend fun guardHousehold(block: suspend () -> Unit) {
        busy {
            if (currentHouseholdId == null) return@busy
            try {
                block()
            } catch (failure: ApiFailure) {
                if (failure.status == 403) {
                    applyAuthenticated(api.me())
                } else {
                    throw failure
                }
            }
        }
    }
}

private fun Throwable.userFacingMessage(): String = when (this) {
    is ApiFailure -> message
    else -> message ?: "Something went wrong."
}
