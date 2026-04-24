package dev.quartermaster.android

import android.content.Context
import android.content.Intent
import android.net.Uri
import android.os.Build
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import dev.quartermaster.android.generated.models.BarcodeLookupResponse
import dev.quartermaster.android.generated.models.CreateInviteRequest
import dev.quartermaster.android.generated.models.CreateStockRequest
import dev.quartermaster.android.generated.models.HouseholdDetailDto
import dev.quartermaster.android.generated.models.InviteDto
import dev.quartermaster.android.generated.models.LocationDto
import dev.quartermaster.android.generated.models.MeResponse
import dev.quartermaster.android.generated.models.MembershipRole
import dev.quartermaster.android.generated.models.ProductDto
import dev.quartermaster.android.generated.models.PushAuthorizationStatus
import dev.quartermaster.android.generated.models.ReminderDto
import dev.quartermaster.android.generated.models.StockBatchDto
import dev.quartermaster.android.generated.models.StockEventDto
import dev.quartermaster.android.generated.models.UnitDto
import java.net.URI
import java.net.URLDecoder
import java.util.UUID

enum class MainTab { Inventory, Reminders, Scan, Settings }

sealed interface AppPhase {
    data object Launching : AppPhase
    data class LaunchFailed(val message: String) : AppPhase
    data object Unauthenticated : AppPhase
    data class Authenticated(val me: MeResponse) : AppPhase
}

enum class LoadState {
    Idle,
    Loading,
}

enum class ReminderAction {
    Open,
    Acknowledge,
}

enum class ScanAction {
    BarcodeLookup,
    ProductSearch,
    AddStock,
}

data class InviteContext(
    val inviteCode: String?,
    val serverUrl: String?,
)

data class InventoryTarget(
    val productId: String,
    val locationId: String,
    val batchId: String? = null,
)

sealed interface HouseholdScopedResolution {
    data object Retry : HouseholdScopedResolution
    data object FallbackToNoHousehold : HouseholdScopedResolution
    data class Failed(val message: String) : HouseholdScopedResolution
}

interface SessionStore {
    fun snapshot(): SessionSnapshot
    fun saveServerUrl(url: String)
    fun saveTokens(accessToken: String, refreshToken: String)
    fun clearTokens()
    fun stableDeviceId(): String
}

interface QuartermasterBackend {
    var serverUrl: String

    suspend fun me(): MeResponse
    suspend fun login(username: String, password: String): Unit
    suspend fun register(username: String, password: String, email: String?, inviteCode: String?): Unit
    suspend fun logout()
    suspend fun switchHousehold(householdId: String): MeResponse
    suspend fun createHousehold(name: String, timezone: String): MeResponse
    suspend fun redeemInvite(inviteCode: String)
    suspend fun currentHousehold(): HouseholdDetailDto
    suspend fun householdInvites(): List<InviteDto>
    suspend fun createInvite(body: CreateInviteRequest): InviteDto
    suspend fun locations(): List<LocationDto>
    suspend fun units(): List<UnitDto>
    suspend fun listStock(): List<StockBatchDto>
    suspend fun listEvents(limit: Int = 30): List<StockEventDto>
    suspend fun listReminders(limit: Int = 50): List<ReminderDto>
    suspend fun acknowledgeReminder(id: String)
    suspend fun presentReminder(id: String)
    suspend fun openReminder(id: String)
    suspend fun registerDevice(
        deviceId: String,
        pushToken: String?,
        authorization: PushAuthorizationStatus,
        appVersion: String,
    )

    suspend fun searchProducts(query: String): List<ProductDto>
    suspend fun lookupBarcode(barcode: String): BarcodeLookupResponse
    suspend fun addStock(request: CreateStockRequest): StockBatchDto
}

class QuartermasterApiBackend(
    private val api: QuartermasterApi,
) : QuartermasterBackend {
    override var serverUrl: String
        get() = api.serverUrl
        set(value) {
            api.serverUrl = value
        }

    override suspend fun me(): MeResponse = api.me()
    override suspend fun login(username: String, password: String) {
        api.login(username = username, password = password)
    }

    override suspend fun register(
        username: String,
        password: String,
        email: String?,
        inviteCode: String?,
    ) {
        api.register(
            username = username,
            password = password,
            email = email,
            inviteCode = inviteCode,
        )
    }

    override suspend fun logout() {
        api.logout()
    }

    override suspend fun switchHousehold(householdId: String): MeResponse = api.switchHousehold(householdId)

    override suspend fun createHousehold(name: String, timezone: String): MeResponse = api.createHousehold(name = name, timezone = timezone)

    override suspend fun redeemInvite(inviteCode: String) {
        api.redeemInvite(inviteCode)
    }

    override suspend fun currentHousehold(): HouseholdDetailDto = api.currentHousehold()
    override suspend fun householdInvites(): List<InviteDto> = api.householdInvites()
    override suspend fun createInvite(body: CreateInviteRequest): InviteDto = api.createInvite(body)
    override suspend fun locations(): List<LocationDto> = api.locations()
    override suspend fun units(): List<UnitDto> = api.units()
    override suspend fun listStock(): List<StockBatchDto> = api.listStock()
    override suspend fun listEvents(limit: Int): List<StockEventDto> = api.listEvents(limit)
    override suspend fun listReminders(limit: Int): List<ReminderDto> = api.listReminders(limit)
    override suspend fun acknowledgeReminder(id: String) = api.acknowledgeReminder(id)
    override suspend fun presentReminder(id: String) = api.presentReminder(id)
    override suspend fun openReminder(id: String) = api.openReminder(id)

    override suspend fun registerDevice(
        deviceId: String,
        pushToken: String?,
        authorization: PushAuthorizationStatus,
        appVersion: String,
    ) {
        api.registerDevice(
            deviceId = deviceId,
            pushToken = pushToken,
            authorization = authorization,
            appVersion = appVersion,
        )
    }

    override suspend fun searchProducts(query: String): List<ProductDto> = api.searchProducts(query)
    override suspend fun lookupBarcode(barcode: String): BarcodeLookupResponse = api.lookupBarcode(barcode)
    override suspend fun addStock(request: CreateStockRequest): StockBatchDto = api.addStock(request)
}

class QuartermasterAppState(
    private val sessionStore: SessionStore,
    private val backend: QuartermasterBackend,
    private val appContext: Context? = null,
) {
    var phase: AppPhase by mutableStateOf(AppPhase.Launching)
        private set
    var selectedTab by mutableStateOf(MainTab.Inventory)
    var lastError by mutableStateOf<String?>(null)
        private set
    var serverUrl by mutableStateOf(sessionStore.snapshot().serverUrl)
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

    var launchState by mutableStateOf(LoadState.Loading)
        private set
    var authActionInFlight by mutableStateOf(false)
        private set
    var hasLoadedInventoryOnce by mutableStateOf(false)
        private set
    var hasLoadedRemindersOnce by mutableStateOf(false)
        private set
    var hasLoadedSettingsOnce by mutableStateOf(false)
        private set
    var inventoryLoadState by mutableStateOf(LoadState.Idle)
        private set
    var remindersLoadState by mutableStateOf(LoadState.Idle)
        private set
    var settingsLoadState by mutableStateOf(LoadState.Idle)
        private set
    var inventoryError by mutableStateOf<String?>(null)
        private set
    var reminderError by mutableStateOf<String?>(null)
        private set
    var settingsError by mutableStateOf<String?>(null)
        private set
    var scanError by mutableStateOf<String?>(null)
        private set

    var pendingInviteContext by mutableStateOf<InviteContext?>(null)
        private set
    var pendingInventoryTarget by mutableStateOf<InventoryTarget?>(null)
        private set
    var shouldRequestNotificationPermission by mutableStateOf(false)
        private set

    private var reminderActionInFlight by mutableStateOf<Map<String, ReminderAction>>(emptyMap())
    var scanActionInFlight by mutableStateOf<ScanAction?>(null)
        private set

    init {
        backend.serverUrl = serverUrl
    }

    suspend fun bootstrap() {
        appContext?.let {
            PushSupport.initialize(it)
            PushSupport.ensureNotificationChannel(it)
        }
        val snapshot = sessionStore.snapshot()
        serverUrl = snapshot.serverUrl
        backend.serverUrl = serverUrl
        if (snapshot.accessToken.isNullOrBlank()) {
            launchState = LoadState.Idle
            phase = AppPhase.Unauthenticated
            return
        }
        runCatching { backend.me() }
            .onSuccess {
                applyAuthenticated(it)
                launchState = LoadState.Idle
            }
            .onFailure {
                sessionStore.clearTokens()
                clearSessionData()
                launchState = LoadState.Idle
                phase = AppPhase.LaunchFailed(it.userFacingMessage())
            }
    }

    fun handleDeepLink(uri: Uri) {
        handleDeepLink(uri.toString())
    }

    suspend fun handleIntent(intent: Intent?) {
        intent?.data?.let(::handleDeepLink)
        val payload = PushSupport.payloadFromIntent(intent) ?: return
        handleReminderPayload(payload)
    }

    suspend fun handleReminderPayload(payload: ReminderPushPayload) {
        pendingInventoryTarget = InventoryTarget(
            productId = payload.productId,
            locationId = payload.locationId,
            batchId = payload.batchId,
        )
        selectedTab = MainTab.Inventory
        if (phase is AppPhase.Authenticated && currentHouseholdId != null) {
            runCatching { backend.openReminder(payload.reminderId) }
            reminders = reminders.filterNot { it.id.toString() == payload.reminderId }
            refreshInventory(force = true)
            refreshReminders(limit = 50)
        }
    }

    fun handleDeepLink(rawUrl: String) {
        parseInviteContext(rawUrl)?.let { context ->
            context.serverUrl?.let(::updateServerUrl)
            pendingInviteContext = context
            selectedTab = MainTab.Settings
        }
    }

    fun updateServerUrl(url: String) {
        serverUrl = url.trim().removeSuffix("/")
        sessionStore.saveServerUrl(serverUrl)
        backend.serverUrl = serverUrl
    }

    suspend fun signIn(username: String, password: String) = runAuthAction {
        backend.login(username = username, password = password)
        applyAuthenticated(backend.me())
    }

    suspend fun register(
        username: String,
        password: String,
        email: String?,
        inviteCode: String?,
    ) = runAuthAction {
        backend.register(
            username = username,
            password = password,
            email = email,
            inviteCode = inviteCode,
        )
        applyAuthenticated(backend.me())
    }

    suspend fun logout() {
        authActionInFlight = true
        lastError = null
        try {
            appContext?.let {
                PushSupport.clearDeviceRegistration(
                    context = it,
                    backend = backend,
                    deviceId = sessionStore.stableDeviceId(),
                )
            }
            backend.logout()
        } catch (_: Throwable) {
            // Best effort.
        } finally {
            authActionInFlight = false
            clearSession()
            phase = AppPhase.Unauthenticated
        }
    }

    suspend fun switchHousehold(householdId: String) = runSettingsAction {
        applyAuthenticated(backend.switchHousehold(householdId))
    }

    suspend fun createHousehold(name: String, timezone: String) = runSettingsAction {
        applyAuthenticated(backend.createHousehold(name = name, timezone = timezone))
    }

    suspend fun redeemInvite(code: String) = runSettingsAction {
        backend.redeemInvite(code)
        applyAuthenticated(backend.me())
    }

    suspend fun createInvite(expiresAt: String, maxUses: Long) = runSettingsAction {
        val invite = backend.createInvite(
            CreateInviteRequest(
                expiresAt = expiresAt,
                maxUses = maxUses,
                roleGranted = MembershipRole.MEMBER,
            ),
        )
        invites = listOf(invite) + invites
    }

    suspend fun refreshInventory(force: Boolean = false) = guardHouseholdScope(
        onStart = {
            inventoryLoadState = LoadState.Loading
            inventoryError = null
        },
        onFailure = { inventoryError = it },
        onFinish = { inventoryLoadState = LoadState.Idle },
    ) {
        if (force || units.isEmpty()) {
            units = backend.units().sortedBy { it.code }
        }
        locations = backend.locations().sortedWith(
            compareBy<LocationDto> { it.sortOrder }.thenBy { it.name.lowercase() },
        )
        batches = backend.listStock().sortedWith(
            compareBy<StockBatchDto> { it.locationId }.thenBy { it.product.name.lowercase() }.thenBy { it.expiresOn ?: "9999-12-31" },
        )
        history = backend.listEvents().sortedByDescending { it.createdAt }
        hasLoadedInventoryOnce = true
    }

    suspend fun refreshReminders(limit: Int = 50) = guardHouseholdScope(
        onStart = {
            remindersLoadState = LoadState.Loading
            reminderError = null
        },
        onFailure = { reminderError = it },
        onFinish = { remindersLoadState = LoadState.Idle },
    ) {
        reminders = backend.listReminders(limit).sortedBy { it.householdFireLocalAt }
        val inFlightIds = reminderActionInFlight.keys
        reminders.filter { it.presentedOnDeviceAt == null && !inFlightIds.contains(it.id.toString()) }.forEach { reminder ->
            runCatching { backend.presentReminder(reminder.id.toString()) }
        }
        hasLoadedRemindersOnce = true
    }

    suspend fun loadSettings() = guardHouseholdScope(
        onStart = {
            settingsLoadState = LoadState.Loading
            settingsError = null
        },
        onFailure = { settingsError = it },
        onFinish = { settingsLoadState = LoadState.Idle },
    ) {
        householdDetail = backend.currentHousehold()
        invites = backend.householdInvites().sortedByDescending { it.createdAt }
        hasLoadedSettingsOnce = true
    }

    suspend fun searchProducts(query: String) {
        runScanAction(ScanAction.ProductSearch) {
            searchResults = if (query.isBlank()) emptyList() else backend.searchProducts(query)
        }
    }

    suspend fun lookupBarcode(barcode: String) {
        runScanAction(ScanAction.BarcodeLookup) {
            val response: BarcodeLookupResponse = backend.lookupBarcode(barcode)
            selectedProduct = response.product
        }
    }

    fun selectProduct(product: ProductDto) {
        selectedProduct = product
        scanError = null
    }

    fun clearInventoryTarget() {
        pendingInventoryTarget = null
    }

    fun clearPendingInviteContext() {
        pendingInviteContext = null
    }

    suspend fun addStock(
        productId: String,
        locationId: String,
        quantity: String,
        unit: String,
        expiresOn: String?,
        note: String?,
    ) = runScanAction(ScanAction.AddStock) {
        backend.addStock(
            CreateStockRequest(
                locationId = UUID.fromString(locationId),
                productId = UUID.fromString(productId),
                quantity = quantity,
                unit = unit,
                expiresOn = expiresOn,
                openedOn = null,
                note = note,
            ),
        )
        searchResults = emptyList()
        selectedProduct = null
        refreshInventory(force = true)
        refreshReminders(limit = 50)
        selectedTab = MainTab.Inventory
    }

    suspend fun acknowledgeReminder(id: String) = runReminderAction(id, ReminderAction.Acknowledge) {
        backend.acknowledgeReminder(id)
        reminders = reminders.filterNot { it.id.toString() == id }
        refreshReminders(limit = 50)
    }

    suspend fun openReminder(reminder: ReminderDto) = runReminderAction(reminder.id.toString(), ReminderAction.Open) {
        backend.openReminder(reminder.id.toString())
        pendingInventoryTarget = InventoryTarget(
            productId = reminder.productId.toString(),
            locationId = reminder.locationId.toString(),
            batchId = reminder.batchId.toString(),
        )
        selectedTab = MainTab.Inventory
        reminders = reminders.filterNot { it.id == reminder.id }
        refreshInventory(force = true)
        refreshReminders(limit = 50)
    }

    suspend fun registerDevice() {
        refreshPushRegistration()
    }

    suspend fun onNotificationPermissionResult(granted: Boolean) {
        shouldRequestNotificationPermission = false
        val context = appContext ?: return
        val authorization = if (granted) {
            PushAuthorizationStatus.AUTHORIZED
        } else {
            PushAuthorizationStatus.DENIED
        }
        PushSupport.syncDeviceRegistration(
            context = context,
            backend = backend,
            deviceId = sessionStore.stableDeviceId(),
            authorizationOverride = authorization,
        )
    }

    fun reminderActionFor(id: String): ReminderAction? = reminderActionInFlight[id]

    fun unitSymbolsFor(product: ProductDto): List<String> = units.filter { it.family == product.family }
        .sortedBy { it.code }
        .map { it.code }

    fun defaultUnitSymbolFor(product: ProductDto): String? {
        val symbols = unitSymbolsFor(product)
        return when {
            product.preferredUnit in symbols -> product.preferredUnit
            else -> symbols.firstOrNull()
        }
    }

    val meOrNull: MeResponse?
        get() = (phase as? AppPhase.Authenticated)?.me

    val currentHouseholdId: String?
        get() = meOrNull?.currentHousehold?.id?.toString()

    val isInventoryRefreshing: Boolean
        get() = inventoryLoadState == LoadState.Loading && hasLoadedInventoryOnce

    val isRemindersRefreshing: Boolean
        get() = remindersLoadState == LoadState.Loading && hasLoadedRemindersOnce

    val isSettingsRefreshing: Boolean
        get() = settingsLoadState == LoadState.Loading && hasLoadedSettingsOnce

    val pendingInviteCode: String?
        get() = pendingInviteContext?.inviteCode

    val hasPendingInviteHandoff: Boolean
        get() = pendingInviteContext != null

    suspend fun resolveHouseholdScopedForbidden(): HouseholdScopedResolution = try {
        val me = backend.me()
        applyAuthenticated(me)
        if (me.currentHousehold != null) {
            HouseholdScopedResolution.Retry
        } else {
            HouseholdScopedResolution.FallbackToNoHousehold
        }
    } catch (failure: Throwable) {
        if (failure is ApiFailure && failure.status == 401) {
            clearSession()
            phase = AppPhase.Unauthenticated
            HouseholdScopedResolution.FallbackToNoHousehold
        } else {
            val message = failure.userFacingMessage()
            lastError = message
            HouseholdScopedResolution.Failed(message)
        }
    }

    private suspend fun applyAuthenticated(me: MeResponse) {
        phase = AppPhase.Authenticated(me)
        lastError = null
        pendingInviteContext = null
        if (me.currentHousehold != null) {
            registerDevice()
            refreshInventory(force = true)
            refreshReminders(limit = 50)
            loadSettings()
        } else {
            clearHouseholdScopedData()
        }
    }

    private suspend fun runAuthAction(block: suspend () -> Unit) {
        authActionInFlight = true
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
            authActionInFlight = false
        }
    }

    private suspend fun runSettingsAction(block: suspend () -> Unit) {
        settingsLoadState = LoadState.Loading
        settingsError = null
        lastError = null
        try {
            block()
        } catch (failure: Throwable) {
            if (failure is ApiFailure && failure.status == 401) {
                clearSession()
                phase = AppPhase.Unauthenticated
            } else {
                val message = failure.userFacingMessage()
                settingsError = message
                lastError = message
            }
        } finally {
            settingsLoadState = LoadState.Idle
        }
    }

    private suspend fun runInventoryMutation(block: suspend () -> Unit) {
        inventoryLoadState = LoadState.Loading
        inventoryError = null
        lastError = null
        try {
            block()
        } catch (failure: Throwable) {
            if (failure is ApiFailure && failure.status == 401) {
                clearSession()
                phase = AppPhase.Unauthenticated
            } else {
                val message = failure.userFacingMessage()
                inventoryError = message
                lastError = message
            }
        } finally {
            inventoryLoadState = LoadState.Idle
        }
    }

    private suspend fun runScanAction(
        action: ScanAction,
        block: suspend () -> Unit,
    ) {
        if (scanActionInFlight != null) return
        scanActionInFlight = action
        scanError = null
        lastError = null
        try {
            block()
        } catch (failure: Throwable) {
            if (failure is ApiFailure && failure.status == 401) {
                clearSession()
                phase = AppPhase.Unauthenticated
            } else {
                val message = failure.userFacingMessage()
                scanError = message
                lastError = message
            }
        } finally {
            scanActionInFlight = null
        }
    }

    private suspend fun runReminderAction(
        id: String,
        action: ReminderAction,
        block: suspend () -> Unit,
    ) {
        if (reminderActionInFlight.containsKey(id)) return
        reminderActionInFlight = reminderActionInFlight + (id to action)
        lastError = null
        try {
            block()
        } catch (failure: Throwable) {
            if (failure is ApiFailure && failure.status == 403) {
                when (resolveHouseholdScopedForbidden()) {
                    HouseholdScopedResolution.Retry -> {
                        reminderActionInFlight = reminderActionInFlight - id
                        runReminderAction(id, action, block)
                        return
                    }
                    HouseholdScopedResolution.FallbackToNoHousehold -> clearHouseholdScopedData()
                    is HouseholdScopedResolution.Failed -> Unit
                }
            } else if (failure is ApiFailure && failure.status == 401) {
                clearSession()
                phase = AppPhase.Unauthenticated
            } else {
                lastError = failure.userFacingMessage()
            }
        } finally {
            reminderActionInFlight = reminderActionInFlight - id
        }
    }

    private suspend fun guardHouseholdScope(
        onStart: () -> Unit,
        onFailure: (String) -> Unit,
        onFinish: () -> Unit,
        block: suspend () -> Unit,
    ) {
        if (currentHouseholdId == null) return
        onStart()
        try {
            block()
        } catch (failure: Throwable) {
            if (failure is ApiFailure && failure.status == 403) {
                when (resolveHouseholdScopedForbidden()) {
                    HouseholdScopedResolution.Retry -> block()
                    HouseholdScopedResolution.FallbackToNoHousehold -> clearHouseholdScopedData()
                    is HouseholdScopedResolution.Failed -> onFailure(failure.userFacingMessage())
                }
            } else if (failure is ApiFailure && failure.status == 401) {
                clearSession()
                phase = AppPhase.Unauthenticated
            } else {
                val message = failure.userFacingMessage()
                onFailure(message)
                lastError = message
            }
        } finally {
            onFinish()
        }
    }

    private fun clearSession() {
        sessionStore.clearTokens()
        clearSessionData()
    }

    private fun clearSessionData() {
        clearHouseholdScopedData()
        searchResults = emptyList()
        selectedProduct = null
        pendingInviteContext = null
        lastError = null
        shouldRequestNotificationPermission = false
    }

    private fun clearHouseholdScopedData() {
        locations = emptyList()
        batches = emptyList()
        reminders = emptyList()
        history = emptyList()
        householdDetail = null
        invites = emptyList()
        pendingInventoryTarget = null
        hasLoadedInventoryOnce = false
        hasLoadedRemindersOnce = false
        hasLoadedSettingsOnce = false
        inventoryError = null
        reminderError = null
        settingsError = null
        scanError = null
        inventoryLoadState = LoadState.Idle
        remindersLoadState = LoadState.Idle
        settingsLoadState = LoadState.Idle
        reminderActionInFlight = emptyMap()
        scanActionInFlight = null
    }

    private suspend fun refreshPushRegistration() {
        val context = appContext ?: return
        if (currentHouseholdId == null || !PushSupport.isFirebaseConfigured()) return
        val authorization = PushSupport.currentAuthorization(context)
        shouldRequestNotificationPermission =
            authorization == PushAuthorizationStatus.NOT_DETERMINED &&
            Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU
        if (authorization != PushAuthorizationStatus.NOT_DETERMINED) {
            PushSupport.syncDeviceRegistration(
                context = context,
                backend = backend,
                deviceId = sessionStore.stableDeviceId(),
                authorizationOverride = authorization,
            )
        }
    }

    companion object {
        fun fromContext(context: Context): QuartermasterAppState {
            val store = AuthStore(context)
            return QuartermasterAppState(
                sessionStore = store,
                backend = QuartermasterApiBackend(QuartermasterApi(store)),
                appContext = context.applicationContext,
            )
        }

        fun parseInviteContext(rawUrl: String): InviteContext? {
            val uri = runCatching { URI(rawUrl) }.getOrNull() ?: return null
            val isJoinLink =
                (uri.scheme == "quartermaster" && uri.host == "join") ||
                    ((uri.scheme == "https" || uri.scheme == "http") && uri.path?.startsWith("/join") == true)
            if (!isJoinLink) return null

            val query = uri.rawQuery.orEmpty()
                .split("&")
                .filter { it.isNotBlank() }
                .mapNotNull { pair ->
                    val parts = pair.split("=", limit = 2)
                    val name = parts.getOrNull(0)?.urlDecode() ?: return@mapNotNull null
                    val value = parts.getOrNull(1)?.urlDecode().orEmpty()
                    name to value
                }
                .toMap()

            val invite = query["invite"]?.trim()?.takeIf { it.isNotEmpty() }
            val server = query["server"]
                ?.trim()
                ?.takeIf { it.startsWith("http://") || it.startsWith("https://") }
                ?.removeSuffix("/")

            return InviteContext(inviteCode = invite, serverUrl = server)
        }
    }
}

private fun Throwable.userFacingMessage(): String = when (this) {
    is ApiFailure -> message
    else -> message ?: "Something went wrong."
}

private fun String.urlDecode(): String = URLDecoder.decode(this, Charsets.UTF_8.name())
