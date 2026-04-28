package dev.quartermaster.android

import android.content.Context
import android.content.Intent
import android.net.Uri
import android.os.Build
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import dev.quartermaster.android.generated.models.BarcodeLookupResponse
import dev.quartermaster.android.generated.models.ConsumeRequest
import dev.quartermaster.android.generated.models.ConsumeResponse
import dev.quartermaster.android.generated.models.CreateInviteRequest
import dev.quartermaster.android.generated.models.CreateLocationRequest
import dev.quartermaster.android.generated.models.CreateProductRequest
import dev.quartermaster.android.generated.models.CreateStockRequest
import dev.quartermaster.android.generated.models.HouseholdDetailDto
import dev.quartermaster.android.generated.models.InviteDto
import dev.quartermaster.android.generated.models.LocationDto
import dev.quartermaster.android.generated.models.MeResponse
import dev.quartermaster.android.generated.models.MembershipRole
import dev.quartermaster.android.generated.models.OnboardingStatusResponse
import dev.quartermaster.android.generated.models.ProductDto
import dev.quartermaster.android.generated.models.ProductSource
import dev.quartermaster.android.generated.models.PushAuthorizationStatus
import dev.quartermaster.android.generated.models.ReminderDto
import dev.quartermaster.android.generated.models.RequestEmailVerificationResponse
import dev.quartermaster.android.generated.models.StockBatchDto
import dev.quartermaster.android.generated.models.StockEventDto
import dev.quartermaster.android.generated.models.StockEventType
import dev.quartermaster.android.generated.models.UnitDto
import dev.quartermaster.android.generated.models.UnitFamily
import dev.quartermaster.android.generated.models.UpdateLocationRequest
import java.net.URI
import java.net.URLDecoder
import java.util.UUID

enum class MainTab { Inventory, Products, Reminders, Scan, Settings }

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

enum class ProductAction {
    LoadList,
    LoadDetail,
    BarcodeLookup,
    Create,
    Update,
    Delete,
    Restore,
    Refresh,
}

enum class LocationAction {
    Create,
    Update,
    Delete,
    Reorder,
}

enum class ProductIncludeFilter {
    Active,
    All,
    Deleted,
}

enum class ScanAction {
    BarcodeLookup,
    ProductSearch,
    AddStock,
}

enum class StockAction {
    LoadEvents,
    Update,
    Consume,
    Discard,
    Restore,
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

data class ProductFormFields(
    val name: String = "",
    val brand: String = "",
    val family: UnitFamily = UnitFamily.MASS,
    val preferredUnit: String = "g",
    val imageUrl: String = "",
)

data class LocationFormFields(
    val name: String = "",
    val kind: String = "pantry",
    val sortOrder: Long? = null,
)

data class StockEditFields(
    val quantity: String = "",
    val locationId: String = "",
    val expiresOn: String = "",
    val openedOn: String = "",
    val note: String = "",
)

data class BatchCounts(
    val active: Int,
    val depleted: Int,
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
    suspend fun onboardingStatus(): OnboardingStatusResponse
    suspend fun createOnboardingHousehold(
        username: String,
        password: String,
        householdName: String,
        timezone: String,
    ): Unit
    suspend fun joinOnboardingInvite(username: String, password: String, inviteCode: String): Unit
    suspend fun login(username: String, password: String): Unit
    suspend fun register(username: String, password: String, inviteCode: String?): Unit
    suspend fun requestEmailVerification(email: String): RequestEmailVerificationResponse
    suspend fun confirmEmailVerification(code: String): MeResponse
    suspend fun clearRecoveryEmail(): MeResponse
    suspend fun logout()
    suspend fun switchHousehold(householdId: String): MeResponse
    suspend fun createHousehold(name: String, timezone: String): MeResponse
    suspend fun redeemInvite(inviteCode: String)
    suspend fun currentHousehold(): HouseholdDetailDto
    suspend fun householdInvites(): List<InviteDto>
    suspend fun createInvite(body: CreateInviteRequest): InviteDto
    suspend fun locations(): List<LocationDto>
    suspend fun createLocation(request: CreateLocationRequest): LocationDto
    suspend fun updateLocation(id: String, request: UpdateLocationRequest): LocationDto
    suspend fun deleteLocation(id: String)
    suspend fun units(): List<UnitDto>
    suspend fun listStock(includeDepleted: Boolean = false): List<StockBatchDto>
    suspend fun listEvents(limit: Int = 30): List<StockEventDto>
    suspend fun listBatchEvents(batchId: String, limit: Int = 30): List<StockEventDto>
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
    suspend fun listProducts(query: String?, limit: Int = 100, includeDeleted: Boolean = false): List<ProductDto>
    suspend fun getProduct(id: String): ProductDto
    suspend fun createProduct(request: CreateProductRequest): ProductDto
    suspend fun updateProduct(id: String, request: ProductUpdateRequest): ProductDto
    suspend fun deleteProduct(id: String)
    suspend fun restoreProduct(id: String): ProductDto
    suspend fun refreshProduct(id: String): ProductDto
    suspend fun lookupBarcode(barcode: String): BarcodeLookupResponse
    suspend fun addStock(request: CreateStockRequest): StockBatchDto
    suspend fun updateStock(id: String, request: StockUpdateRequest): StockBatchDto
    suspend fun consumeStock(request: ConsumeRequest): ConsumeResponse
    suspend fun discardStock(batchId: String)
    suspend fun restoreStock(batchId: String): StockBatchDto
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
    override suspend fun onboardingStatus(): OnboardingStatusResponse = api.onboardingStatus()
    override suspend fun createOnboardingHousehold(
        username: String,
        password: String,
        householdName: String,
        timezone: String,
    ) {
        api.createOnboardingHousehold(
            username = username,
            password = password,
            householdName = householdName,
            timezone = timezone,
        )
    }

    override suspend fun joinOnboardingInvite(username: String, password: String, inviteCode: String) {
        api.joinOnboardingInvite(username = username, password = password, inviteCode = inviteCode)
    }

    override suspend fun login(username: String, password: String) {
        api.login(username = username, password = password)
    }

    override suspend fun register(
        username: String,
        password: String,
        inviteCode: String?,
    ) {
        api.register(
            username = username,
            password = password,
            inviteCode = inviteCode,
        )
    }

    override suspend fun requestEmailVerification(email: String): RequestEmailVerificationResponse = api.requestEmailVerification(email)

    override suspend fun confirmEmailVerification(code: String): MeResponse = api.confirmEmailVerification(code)

    override suspend fun clearRecoveryEmail(): MeResponse = api.clearRecoveryEmail()

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
    override suspend fun createLocation(request: CreateLocationRequest): LocationDto = api.createLocation(request)
    override suspend fun updateLocation(id: String, request: UpdateLocationRequest): LocationDto = api.updateLocation(id, request)
    override suspend fun deleteLocation(id: String) = api.deleteLocation(id)
    override suspend fun units(): List<UnitDto> = api.units()
    override suspend fun listStock(includeDepleted: Boolean): List<StockBatchDto> = api.listStock(includeDepleted)
    override suspend fun listEvents(limit: Int): List<StockEventDto> = api.listEvents(limit)
    override suspend fun listBatchEvents(batchId: String, limit: Int): List<StockEventDto> = api.listBatchEvents(batchId, limit)
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
    override suspend fun listProducts(query: String?, limit: Int, includeDeleted: Boolean): List<ProductDto> = api.listProducts(query = query, limit = limit, includeDeleted = includeDeleted)
    override suspend fun getProduct(id: String): ProductDto = api.getProduct(id)
    override suspend fun createProduct(request: CreateProductRequest): ProductDto = api.createProduct(request)
    override suspend fun updateProduct(id: String, request: ProductUpdateRequest): ProductDto = api.updateProduct(id, request)
    override suspend fun deleteProduct(id: String) = api.deleteProduct(id)
    override suspend fun restoreProduct(id: String): ProductDto = api.restoreProduct(id)
    override suspend fun refreshProduct(id: String): ProductDto = api.refreshProduct(id)
    override suspend fun lookupBarcode(barcode: String): BarcodeLookupResponse = api.lookupBarcode(barcode)
    override suspend fun addStock(request: CreateStockRequest): StockBatchDto = api.addStock(request)
    override suspend fun updateStock(id: String, request: StockUpdateRequest): StockBatchDto = api.updateStock(id, request)
    override suspend fun consumeStock(request: ConsumeRequest): ConsumeResponse = api.consumeStock(request)
    override suspend fun discardStock(batchId: String) = api.discardStock(batchId)
    override suspend fun restoreStock(batchId: String): StockBatchDto = api.restoreStock(batchId)
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
    var products by mutableStateOf<List<ProductDto>>(emptyList())
        private set
    var productSearchQuery by mutableStateOf("")
        private set
    var productIncludeFilter by mutableStateOf(ProductIncludeFilter.Active)
        private set
    var selectedCatalogueProduct by mutableStateOf<ProductDto?>(null)
        private set
    var selectedBatchId by mutableStateOf<String?>(null)
        private set
    var selectedBatchEvents by mutableStateOf<List<StockEventDto>>(emptyList())
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
    var hasLoadedProductsOnce by mutableStateOf(false)
        private set
    var hasLoadedRemindersOnce by mutableStateOf(false)
        private set
    var hasLoadedSettingsOnce by mutableStateOf(false)
        private set
    var inventoryLoadState by mutableStateOf(LoadState.Idle)
        private set
    var productLoadState by mutableStateOf(LoadState.Idle)
        private set
    var remindersLoadState by mutableStateOf(LoadState.Idle)
        private set
    var settingsLoadState by mutableStateOf(LoadState.Idle)
        private set
    var inventoryError by mutableStateOf<String?>(null)
        private set
    var productError by mutableStateOf<String?>(null)
        private set
    var reminderError by mutableStateOf<String?>(null)
        private set
    var settingsError by mutableStateOf<String?>(null)
        private set
    var selectedBatchEventError by mutableStateOf<String?>(null)
        private set
    var scanError by mutableStateOf<String?>(null)
        private set

    var pendingInviteContext by mutableStateOf<InviteContext?>(null)
        private set
    var onboardingStatus by mutableStateOf<OnboardingStatusResponse?>(null)
        private set
    var pendingInventoryTarget by mutableStateOf<InventoryTarget?>(null)
        private set
    var shouldRequestNotificationPermission by mutableStateOf(false)
        private set

    private var reminderActionInFlight by mutableStateOf<Map<String, ReminderAction>>(emptyMap())
    var productActionInFlight by mutableStateOf<ProductAction?>(null)
        private set
    var locationActionInFlight by mutableStateOf<LocationAction?>(null)
        private set
    private var stockActionInFlight by mutableStateOf<Map<String, StockAction>>(emptyMap())
    var scanActionInFlight by mutableStateOf<ScanAction?>(null)
        private set
    var returnToScanAfterProductCreate by mutableStateOf(false)
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
            if (phase is AppPhase.Unauthenticated) {
                context.serverUrl?.let(::updateServerUrl)
            }
            if (!context.inviteCode.isNullOrBlank()) {
                pendingInviteContext = context
                selectedTab = MainTab.Settings
            }
        }
    }

    suspend fun refreshOnboardingStatus() = runAuthAction {
        onboardingStatus = backend.onboardingStatus()
    }

    fun updateServerUrl(url: String) {
        serverUrl = url.trim().removeSuffix("/")
        sessionStore.saveServerUrl(serverUrl)
        backend.serverUrl = serverUrl
    }

    fun clearOnboardingStatus() {
        onboardingStatus = null
    }

    suspend fun signIn(username: String, password: String) = runAuthAction {
        backend.login(username = username, password = password)
        applyAuthenticated(backend.me())
    }

    suspend fun createOnboardingHousehold(
        username: String,
        password: String,
        householdName: String,
        timezone: String,
    ) = runAuthAction {
        backend.createOnboardingHousehold(
            username = username,
            password = password,
            householdName = householdName,
            timezone = timezone,
        )
        applyAuthenticated(backend.me())
    }

    suspend fun joinOnboardingInvite(username: String, password: String, inviteCode: String) = runAuthAction {
        backend.joinOnboardingInvite(username = username, password = password, inviteCode = inviteCode)
        pendingInviteContext = null
        applyAuthenticated(backend.me())
    }

    suspend fun register(
        username: String,
        password: String,
        inviteCode: String?,
    ) = runAuthAction {
        backend.register(
            username = username,
            password = password,
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

    suspend fun requestEmailVerification(email: String) = runSettingsAction {
        backend.requestEmailVerification(email.trim())
        applyAuthenticated(backend.me())
    }

    suspend fun confirmEmailVerification(code: String) = runSettingsAction {
        applyAuthenticated(backend.confirmEmailVerification(code.trim()))
    }

    suspend fun clearRecoveryEmail() = runSettingsAction {
        applyAuthenticated(backend.clearRecoveryEmail())
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

    suspend fun createLocation(fields: LocationFormFields): Boolean {
        validateLocationForm(fields)?.let {
            settingsError = it
            lastError = it
            return false
        }
        var saved = false
        runLocationAction(LocationAction.Create) {
            backend.createLocation(
                CreateLocationRequest(
                    name = fields.name.trim(),
                    kind = fields.kind,
                    sortOrder = fields.sortOrder,
                ),
            )
            refreshLocationsAndInventory()
            saved = true
        }
        return saved
    }

    suspend fun updateLocation(
        id: String,
        fields: LocationFormFields,
    ): Boolean {
        validateLocationForm(fields)?.let {
            settingsError = it
            lastError = it
            return false
        }
        val sortOrder = fields.sortOrder ?: locations.firstOrNull { it.id.toString() == id }?.sortOrder ?: return false
        var saved = false
        runLocationAction(LocationAction.Update) {
            backend.updateLocation(
                id,
                UpdateLocationRequest(
                    name = fields.name.trim(),
                    kind = fields.kind,
                    sortOrder = sortOrder,
                ),
            )
            refreshLocationsAndInventory()
            saved = true
        }
        return saved
    }

    suspend fun deleteLocation(id: String): Boolean {
        var deleted = false
        runLocationAction(LocationAction.Delete) {
            backend.deleteLocation(id)
            refreshLocationsAndInventory()
            deleted = true
        }
        return deleted
    }

    suspend fun moveLocation(
        id: String,
        delta: Int,
    ) {
        val sorted = sortedLocations()
        val index = sorted.indexOfFirst { it.id.toString() == id }
        val targetIndex = index + delta
        if (index !in sorted.indices || targetIndex !in sorted.indices) return
        val current = sorted[index]
        val neighbor = sorted[targetIndex]
        runLocationAction(LocationAction.Reorder) {
            backend.updateLocation(
                current.id.toString(),
                UpdateLocationRequest(
                    name = current.name,
                    kind = current.kind,
                    sortOrder = neighbor.sortOrder,
                ),
            )
            backend.updateLocation(
                neighbor.id.toString(),
                UpdateLocationRequest(
                    name = neighbor.name,
                    kind = neighbor.kind,
                    sortOrder = current.sortOrder,
                ),
            )
            refreshLocationsAndInventory()
        }
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
        locations = sortLocations(backend.locations())
        batches = backend.listStock(includeDepleted = true).sortedWith(
            compareBy<StockBatchDto> { isBatchDepleted(it) }
                .thenBy { it.locationId }
                .thenBy { it.product.name.lowercase() }
                .thenBy { it.expiresOn ?: "9999-12-31" },
        )
        history = backend.listEvents().sortedByDescending { it.createdAt }
        selectedBatchId?.let { id ->
            if (batches.none { it.id.toString() == id }) {
                clearSelectedBatch()
            }
        }
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
        reminders = sortReminders(backend.listReminders(limit))
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

    suspend fun refreshProducts(force: Boolean = false) = guardHouseholdScope(
        onStart = {
            productLoadState = LoadState.Loading
            productError = null
            productActionInFlight = ProductAction.LoadList
        },
        onFailure = { productError = it },
        onFinish = {
            productLoadState = LoadState.Idle
            productActionInFlight = null
        },
    ) {
        refreshProductsBody(forceUnits = force)
        hasLoadedProductsOnce = true
    }

    suspend fun applyProductFilters(
        query: String,
        filter: ProductIncludeFilter,
    ) {
        productSearchQuery = query
        productIncludeFilter = filter
        refreshProducts(force = false)
    }

    suspend fun openProduct(id: String): ProductDto? {
        var product: ProductDto? = null
        runProductAction(ProductAction.LoadDetail) {
            product = backend.getProduct(id)
            selectedCatalogueProduct = product
        }
        return product
    }

    suspend fun lookupProductBarcode(barcode: String): ProductDto? {
        var product: ProductDto? = null
        runProductAction(ProductAction.BarcodeLookup) {
            val response = backend.lookupBarcode(barcode)
            product = response.product
            selectedCatalogueProduct = response.product
            products = upsertProduct(products, response.product)
        }
        return product
    }

    fun prepareProductList() {
        selectedCatalogueProduct = null
        productError = null
    }

    fun prepareProductCreate() {
        returnToScanAfterProductCreate = false
        selectedCatalogueProduct = null
        productError = null
    }

    fun prepareProductCreateForScan() {
        returnToScanAfterProductCreate = true
        selectedCatalogueProduct = null
        productError = null
        selectedTab = MainTab.Products
    }

    fun cancelProductFormForScan(): Boolean {
        if (returnToScanAfterProductCreate) {
            returnToScanAfterProductCreate = false
            selectedCatalogueProduct = null
            productError = null
            selectedTab = MainTab.Scan
            return true
        }
        return false
    }

    fun prepareProductDetail() {
        productError = null
    }

    suspend fun createProduct(fields: ProductFormFields): ProductDto? {
        validateProductForm(fields)?.let {
            productError = it
            lastError = it
            return null
        }
        var created: ProductDto? = null
        runProductAction(ProductAction.Create) {
            val product = backend.createProduct(fields.toCreateProductRequest())
            created = product
            selectedCatalogueProduct = product
            products = upsertProduct(products, product)
            if (returnToScanAfterProductCreate) {
                selectedProduct = product
                searchResults = emptyList()
                selectedCatalogueProduct = null
                selectedTab = MainTab.Scan
                returnToScanAfterProductCreate = false
            }
            refreshProductsBody(forceUnits = false)
            hasLoadedProductsOnce = true
        }
        return created
    }

    suspend fun updateSelectedProduct(fields: ProductFormFields): ProductDto? {
        val product = selectedCatalogueProduct ?: return null
        validateProductForm(fields)?.let {
            productError = it
            lastError = it
            return null
        }
        val patch = fields.toUpdatePatch(product)
        var updatedProduct: ProductDto? = null
        runProductAction(ProductAction.Update) {
            val updated = backend.updateProduct(product.id.toString(), patch)
            updatedProduct = updated
            selectedCatalogueProduct = updated
            products = upsertProduct(products, updated)
            refreshProductsBody(forceUnits = false)
            hasLoadedProductsOnce = true
        }
        return updatedProduct
    }

    suspend fun deleteSelectedProduct(): Boolean {
        val product = selectedCatalogueProduct ?: return false
        var deleted = false
        runProductAction(ProductAction.Delete) {
            backend.deleteProduct(product.id.toString())
            selectedCatalogueProduct = null
            refreshProductsBody(forceUnits = false)
            hasLoadedProductsOnce = true
            deleted = true
        }
        return deleted
    }

    suspend fun restoreSelectedProduct(): ProductDto? {
        val product = selectedCatalogueProduct ?: return null
        var restoredProduct: ProductDto? = null
        runProductAction(ProductAction.Restore) {
            val restored = backend.restoreProduct(product.id.toString())
            restoredProduct = restored
            selectedCatalogueProduct = restored
            products = upsertProduct(products, restored)
            refreshProductsBody(forceUnits = false)
            hasLoadedProductsOnce = true
        }
        return restoredProduct
    }

    suspend fun refreshSelectedProductFromOff(): ProductDto? {
        val product = selectedCatalogueProduct ?: return null
        var refreshedProduct: ProductDto? = null
        runProductAction(ProductAction.Refresh) {
            val refreshed = backend.refreshProduct(product.id.toString())
            refreshedProduct = refreshed
            selectedCatalogueProduct = refreshed
            products = upsertProduct(products, refreshed)
            refreshProductsBody(forceUnits = false)
            hasLoadedProductsOnce = true
        }
        return refreshedProduct
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

    fun clearSelectedBatch() {
        selectedBatchId = null
        selectedBatchEvents = emptyList()
        selectedBatchEventError = null
    }

    fun clearPendingInviteContext() {
        pendingInviteContext = null
    }

    suspend fun selectBatch(batchId: String) {
        selectedBatchId = batchId
        selectedBatchEventError = null
        loadSelectedBatchEvents()
    }

    suspend fun loadSelectedBatchEvents(limit: Int = 30) {
        val batchId = selectedBatchId ?: return
        runStockAction(batchId, StockAction.LoadEvents) {
            refreshSelectedBatchEvents(limit)
        }
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

    suspend fun consumeSelectedBatch(quantity: String) {
        val batch = selectedBatch ?: return
        runStockAction(batch.id.toString(), StockAction.Consume) {
            backend.consumeStock(
                ConsumeRequest(
                    productId = batch.product.id,
                    quantity = quantity,
                    unit = batch.unit,
                    locationId = batch.locationId,
                ),
            )
            refreshInventoryAfterStockMutation(batch.id.toString())
        }
    }

    suspend fun updateSelectedBatch(fields: StockEditFields): Boolean {
        val batch = selectedBatch ?: return false
        if (!canEditBatch(batch)) return false
        validateStockEditFields(fields)?.let {
            inventoryError = it
            lastError = it
            return false
        }
        val patch = stockUpdateRequest(batch, fields)
        if (patch.operations.isEmpty()) return true
        var saved = false
        runStockAction(batch.id.toString(), StockAction.Update) {
            backend.updateStock(batch.id.toString(), patch)
            refreshInventoryAfterStockMutation(batch.id.toString())
            saved = true
        }
        return saved
    }

    suspend fun discardBatch(batchId: String) = runStockAction(batchId, StockAction.Discard) {
        backend.discardStock(batchId)
        refreshInventoryAfterStockMutation(batchId)
    }

    suspend fun restoreBatch(batchId: String) = runStockAction(batchId, StockAction.Restore) {
        backend.restoreStock(batchId)
        refreshInventoryAfterStockMutation(batchId)
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

    fun stockActionFor(id: String): StockAction? = stockActionInFlight[id]

    val selectedBatch: StockBatchDto?
        get() = selectedBatchId?.let { id -> batches.firstOrNull { it.id.toString() == id } }

    val selectedBatchEventLoadState: LoadState
        get() = if (selectedBatchId?.let { stockActionInFlight[it] } == StockAction.LoadEvents) LoadState.Loading else LoadState.Idle

    fun locationNameFor(locationId: String): String = locations.firstOrNull { it.id.toString() == locationId }?.name ?: "Unknown location"

    fun locationNameFor(batch: StockBatchDto): String = batch.locationName
        .takeIf { it.isNotBlank() }
        ?: locationNameFor(batch.locationId.toString())

    fun sortedLocations(): List<LocationDto> = sortLocations(locations)

    fun locationFormFields(location: LocationDto): LocationFormFields = LocationFormFields(
        name = location.name,
        kind = location.kind,
        sortOrder = location.sortOrder,
    )

    fun stockEditFields(batch: StockBatchDto): StockEditFields = StockEditFields(
        quantity = batch.quantity,
        locationId = batch.locationId.toString(),
        expiresOn = batch.expiresOn.orEmpty(),
        openedOn = batch.openedOn.orEmpty(),
        note = batch.note.orEmpty(),
    )

    fun isBatchDepleted(batch: StockBatchDto): Boolean = batch.depletedAt != null

    fun batchCountsForLocation(locationId: String): BatchCounts {
        val inLocation = batches.filter { it.locationId.toString() == locationId }
        return BatchCounts(
            active = inLocation.count { !isBatchDepleted(it) },
            depleted = inLocation.count { isBatchDepleted(it) },
        )
    }

    fun sortReminders(items: List<ReminderDto>): List<ReminderDto> = items.sortedWith(
        compareBy<ReminderDto>(
            { it.daysUntilExpiry ?: Long.MAX_VALUE },
            { it.expiresOn ?: "9999-12-31" },
            { it.householdFireLocalAt },
            { it.id.toString() },
        ),
    )

    fun canEditBatch(batch: StockBatchDto?): Boolean = batch != null && !isBatchDepleted(batch)

    fun canRestoreBatch(batch: StockBatchDto?): Boolean {
        if (batch == null || !isBatchDepleted(batch) || selectedBatchId != batch.id.toString()) return false
        return selectedBatchEvents.firstOrNull()?.eventType == StockEventType.DISCARD
    }

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

    fun productUnitSymbolsFor(family: UnitFamily): List<String> {
        val fromServer = units.filter { it.family == family }
            .sortedBy { it.code }
            .map { it.code }
        return fromServer.ifEmpty { defaultUnitSymbolsForFamily(family) }
    }

    fun defaultProductUnitFor(family: UnitFamily): String = productUnitSymbolsFor(family).first()

    fun productFormFields(product: ProductDto): ProductFormFields = ProductFormFields(
        name = product.name,
        brand = product.brand.orEmpty(),
        family = product.family,
        preferredUnit = product.preferredUnit,
        imageUrl = product.imageUrl.orEmpty(),
    )

    fun productFormWithFamily(
        fields: ProductFormFields,
        family: UnitFamily,
    ): ProductFormFields = fields.copy(
        family = family,
        preferredUnit = defaultProductUnitFor(family),
    )

    fun visibleProducts(): List<ProductDto> = when (productIncludeFilter) {
        ProductIncludeFilter.Active -> products.filterNot { it.isDeletedProduct() }
        ProductIncludeFilter.All -> products
        ProductIncludeFilter.Deleted -> products.filter { it.isDeletedProduct() }
    }

    fun isManualProduct(product: ProductDto): Boolean = product.source == ProductSource.MANUAL

    fun isDeletedProduct(product: ProductDto): Boolean = product.isDeletedProduct()

    fun productSourceLabel(product: ProductDto): String = if (product.source == ProductSource.OPENFOODFACTS) "OpenFoodFacts" else "Manual"

    fun validateProductForm(fields: ProductFormFields): String? {
        val name = fields.name.trim()
        return when {
            name.isEmpty() -> "Enter a product name."
            name.length > 256 -> "Product name must be 256 characters or fewer."
            fields.preferredUnit !in productUnitSymbolsFor(fields.family) -> "Choose a preferred unit that matches the product family."
            else -> null
        }
    }

    fun validateLocationForm(fields: LocationFormFields): String? {
        val name = fields.name.trim()
        return when {
            name.isEmpty() -> "Enter a location name."
            name.length > 64 -> "Location name must be 64 characters or fewer."
            fields.kind !in LOCATION_KINDS -> "Choose pantry, fridge, or freezer."
            else -> null
        }
    }

    fun validateStockEditFields(fields: StockEditFields): String? {
        val quantity = fields.quantity.trim()
        val expiresOn = fields.expiresOn.trim()
        val openedOn = fields.openedOn.trim()
        return when {
            quantity.isEmpty() -> "Enter a quantity."
            quantity.toBigDecimalOrNull()?.let { it > java.math.BigDecimal.ZERO } != true -> "Enter a positive quantity."
            fields.locationId.isBlank() -> "Choose a location."
            locations.none { it.id.toString() == fields.locationId } -> "Choose an existing location."
            expiresOn.isNotEmpty() && !LOCAL_DATE.matches(expiresOn) -> "Enter expiry as YYYY-MM-DD."
            openedOn.isNotEmpty() && !LOCAL_DATE.matches(openedOn) -> "Enter opened date as YYYY-MM-DD."
            else -> null
        }
    }

    fun stockUpdateRequest(
        batch: StockBatchDto,
        fields: StockEditFields,
    ): StockUpdateRequest = fields.toUpdatePatch(batch)

    val meOrNull: MeResponse?
        get() = (phase as? AppPhase.Authenticated)?.me

    val currentHouseholdId: String?
        get() = meOrNull?.currentHousehold?.id?.toString()

    val isInventoryRefreshing: Boolean
        get() = inventoryLoadState == LoadState.Loading && hasLoadedInventoryOnce

    val isProductsRefreshing: Boolean
        get() = productLoadState == LoadState.Loading && hasLoadedProductsOnce

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
            refreshProducts(force = true)
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

    private suspend fun runStockAction(
        batchId: String,
        action: StockAction,
        block: suspend () -> Unit,
    ) {
        if (stockActionInFlight.containsKey(batchId)) return
        stockActionInFlight = stockActionInFlight + (batchId to action)
        if (action == StockAction.LoadEvents) {
            selectedBatchEventError = null
        } else {
            inventoryError = null
        }
        lastError = null
        try {
            block()
        } catch (failure: Throwable) {
            if (failure is ApiFailure && failure.status == 403) {
                when (resolveHouseholdScopedForbidden()) {
                    HouseholdScopedResolution.Retry -> {
                        stockActionInFlight = stockActionInFlight - batchId
                        runStockAction(batchId, action, block)
                        return
                    }
                    HouseholdScopedResolution.FallbackToNoHousehold -> clearHouseholdScopedData()
                    is HouseholdScopedResolution.Failed -> Unit
                }
            } else if (failure is ApiFailure && failure.status == 401) {
                clearSession()
                phase = AppPhase.Unauthenticated
            } else {
                val message = failure.userFacingMessage()
                if (action == StockAction.LoadEvents) {
                    selectedBatchEventError = message
                } else {
                    inventoryError = message
                }
                lastError = message
            }
        } finally {
            stockActionInFlight = stockActionInFlight - batchId
        }
    }

    private suspend fun refreshInventoryAfterStockMutation(batchId: String) {
        selectedBatchId = batchId
        refreshInventory(force = true)
        refreshReminders(limit = 50)
        if (selectedBatchId == batchId) {
            refreshSelectedBatchEvents()
        }
    }

    private suspend fun refreshSelectedBatchEvents(limit: Int = 30) {
        val batchId = selectedBatchId ?: return
        selectedBatchEvents = backend.listBatchEvents(batchId, limit).sortedByDescending { it.createdAt }
    }

    private suspend fun refreshLocationsAndInventory() {
        locations = sortLocations(backend.locations())
        refreshInventory(force = true)
    }

    private suspend fun runLocationAction(
        action: LocationAction,
        block: suspend () -> Unit,
    ) {
        if (locationActionInFlight != null) return
        locationActionInFlight = action
        settingsError = null
        lastError = null
        try {
            block()
        } catch (failure: Throwable) {
            if (failure is ApiFailure && failure.status == 403) {
                when (resolveHouseholdScopedForbidden()) {
                    HouseholdScopedResolution.Retry -> {
                        locationActionInFlight = null
                        runLocationAction(action, block)
                        return
                    }
                    HouseholdScopedResolution.FallbackToNoHousehold -> clearHouseholdScopedData()
                    is HouseholdScopedResolution.Failed -> Unit
                }
            } else if (failure is ApiFailure && failure.status == 401) {
                clearSession()
                phase = AppPhase.Unauthenticated
            } else {
                val message = failure.userFacingMessage()
                settingsError = message
                lastError = message
            }
        } finally {
            locationActionInFlight = null
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

    private suspend fun runProductAction(
        action: ProductAction,
        block: suspend () -> Unit,
    ) {
        if (productActionInFlight != null) return
        productActionInFlight = action
        productError = null
        lastError = null
        try {
            block()
        } catch (failure: Throwable) {
            if (failure is ApiFailure && failure.status == 403) {
                when (resolveHouseholdScopedForbidden()) {
                    HouseholdScopedResolution.Retry -> {
                        productActionInFlight = null
                        runProductAction(action, block)
                        return
                    }
                    HouseholdScopedResolution.FallbackToNoHousehold -> clearHouseholdScopedData()
                    is HouseholdScopedResolution.Failed -> Unit
                }
            } else if (failure is ApiFailure && failure.status == 401) {
                clearSession()
                phase = AppPhase.Unauthenticated
            } else {
                val message = failure.productFacingMessage(action)
                productError = message
                lastError = message
            }
        } finally {
            productActionInFlight = null
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

    private suspend fun refreshProductsBody(forceUnits: Boolean) {
        if (forceUnits || units.isEmpty()) {
            units = backend.units().sortedBy { it.code }
        }
        products = backend.listProducts(
            query = productSearchQuery.trim().takeIf(String::isNotEmpty),
            limit = 100,
            includeDeleted = productIncludeFilter != ProductIncludeFilter.Active,
        ).sortedWith(
            compareBy<ProductDto> { it.isDeletedProduct() }
                .thenBy { it.name.lowercase() }
                .thenBy { it.brand.orEmpty().lowercase() },
        )
        selectedCatalogueProduct?.let { selected ->
            selectedCatalogueProduct = products.firstOrNull { it.id == selected.id } ?: selected
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
        products = emptyList()
        productSearchQuery = ""
        productIncludeFilter = ProductIncludeFilter.Active
        selectedCatalogueProduct = null
        clearSelectedBatch()
        householdDetail = null
        invites = emptyList()
        pendingInventoryTarget = null
        hasLoadedInventoryOnce = false
        hasLoadedProductsOnce = false
        hasLoadedRemindersOnce = false
        hasLoadedSettingsOnce = false
        inventoryError = null
        productError = null
        reminderError = null
        settingsError = null
        scanError = null
        inventoryLoadState = LoadState.Idle
        productLoadState = LoadState.Idle
        remindersLoadState = LoadState.Idle
        settingsLoadState = LoadState.Idle
        reminderActionInFlight = emptyMap()
        productActionInFlight = null
        stockActionInFlight = emptyMap()
        scanActionInFlight = null
        locationActionInFlight = null
        returnToScanAfterProductCreate = false
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
        private val LOCATION_KINDS = setOf("pantry", "fridge", "freezer")
        private val LOCAL_DATE = Regex("""\d{4}-\d{2}-\d{2}""")

        private fun sortLocations(locations: List<LocationDto>): List<LocationDto> = locations.sortedWith(
            compareBy<LocationDto> { it.sortOrder }.thenBy { it.name.lowercase() },
        )

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
            val isServerLink = uri.scheme == "quartermaster" && uri.host == "server"
            val isJoinLink =
                (uri.scheme == "quartermaster" && uri.host == "join") ||
                    ((uri.scheme == "https" || uri.scheme == "http") && uri.path?.startsWith("/join") == true)
            if (!isServerLink && !isJoinLink) return null

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

            return InviteContext(inviteCode = if (isJoinLink) invite else null, serverUrl = server)
        }
    }
}

private fun Throwable.userFacingMessage(): String = when (this) {
    is ApiFailure -> message
    else -> message ?: "Something went wrong."
}

private fun Throwable.productFacingMessage(action: ProductAction): String {
    if (this !is ApiFailure) return userFacingMessage()
    if (action == ProductAction.BarcodeLookup) {
        return when (status) {
            400 -> "Enter an EAN-8, UPC-A, EAN-13, or EAN-14 barcode."
            404 -> "No product was found for that barcode."
            429 -> "Barcode lookup is rate-limited. Try again shortly."
            502 -> "Barcode lookup is temporarily unavailable."
            else -> message.ifBlank { "Barcode lookup failed." }
        }
    }
    return when (code) {
        "off_product_read_only" -> "OpenFoodFacts products are read-only from the Android catalogue."
        "product_has_stock" -> "This product still has active stock. Consume or discard it first."
        "product_has_incompatible_stock" -> "This product has active stock with units that do not fit the new family."
        "unit_family_mismatch",
        "unknown_unit",
        -> "Choose a unit that matches the product family."
        "not_found" -> "Product could not be found."
        else -> message
    }
}

private fun ProductDto.isDeletedProduct(): Boolean = !deletedAt.isNullOrBlank()

private fun ProductDto.isEditableManualProduct(): Boolean = source == ProductSource.MANUAL && !isDeletedProduct()

private fun defaultUnitSymbolsForFamily(family: UnitFamily): List<String> = when (family) {
    UnitFamily.MASS -> listOf("g", "kg")
    UnitFamily.VOLUME -> listOf("ml", "l")
    UnitFamily.COUNT -> listOf("piece")
}

private fun ProductFormFields.toCreateProductRequest(): CreateProductRequest = CreateProductRequest(
    name = name.trim(),
    brand = brand.trim().takeIf(String::isNotEmpty),
    family = family,
    preferredUnit = preferredUnit,
    barcode = null,
    imageUrl = imageUrl.trim().takeIf(String::isNotEmpty),
)

private fun ProductFormFields.toUpdatePatch(product: ProductDto): ProductUpdateRequest {
    val nextName = name.trim()
    val nextBrand = brand.trim()
    val currentBrand = product.brand.orEmpty()
    val nextImageUrl = imageUrl.trim()
    val currentImageUrl = product.imageUrl.orEmpty()
    val operations = buildList {
        if (nextName != product.name) add(JsonPatchOperation("replace", "/name", nextName))
        when {
            nextBrand.isNotEmpty() && nextBrand != currentBrand -> add(JsonPatchOperation("replace", "/brand", nextBrand))
            nextBrand.isEmpty() && currentBrand.isNotEmpty() -> add(JsonPatchOperation("remove", "/brand"))
        }
        if (family != product.family) add(JsonPatchOperation("replace", "/family", family.value))
        if (preferredUnit != product.preferredUnit) {
            add(JsonPatchOperation("replace", "/preferred_unit", preferredUnit))
        }
        when {
            nextImageUrl.isNotEmpty() && nextImageUrl != currentImageUrl ->
                add(JsonPatchOperation("replace", "/image_url", nextImageUrl))
            nextImageUrl.isEmpty() && currentImageUrl.isNotEmpty() -> add(JsonPatchOperation("remove", "/image_url"))
        }
    }
    return ProductUpdateRequest(operations)
}

private fun StockEditFields.toUpdatePatch(batch: StockBatchDto): StockUpdateRequest {
    val nextQuantity = quantity.trim()
    val nextLocationId = locationId.trim()
    val nextExpiresOn = expiresOn.trim()
    val nextOpenedOn = openedOn.trim()
    val nextNote = note.trim()
    val currentExpiresOn = batch.expiresOn.orEmpty()
    val currentOpenedOn = batch.openedOn.orEmpty()
    val currentNote = batch.note.orEmpty()
    val operations = buildList {
        if (nextQuantity != batch.quantity) add(JsonPatchOperation("replace", "/quantity", nextQuantity))
        if (nextLocationId != batch.locationId.toString()) add(JsonPatchOperation("replace", "/location_id", nextLocationId))
        when {
            nextExpiresOn.isNotEmpty() && nextExpiresOn != currentExpiresOn -> add(JsonPatchOperation("replace", "/expires_on", nextExpiresOn))
            nextExpiresOn.isEmpty() && currentExpiresOn.isNotEmpty() -> add(JsonPatchOperation("remove", "/expires_on"))
        }
        when {
            nextOpenedOn.isNotEmpty() && nextOpenedOn != currentOpenedOn -> add(JsonPatchOperation("replace", "/opened_on", nextOpenedOn))
            nextOpenedOn.isEmpty() && currentOpenedOn.isNotEmpty() -> add(JsonPatchOperation("remove", "/opened_on"))
        }
        when {
            nextNote.isNotEmpty() && nextNote != currentNote -> add(JsonPatchOperation("replace", "/note", nextNote))
            nextNote.isEmpty() && currentNote.isNotEmpty() -> add(JsonPatchOperation("remove", "/note"))
        }
    }
    return StockUpdateRequest(operations)
}

private fun upsertProduct(
    products: List<ProductDto>,
    product: ProductDto,
): List<ProductDto> {
    val without = products.filterNot { it.id == product.id }
    return (listOf(product) + without).sortedWith(
        compareBy<ProductDto> { it.isDeletedProduct() }
            .thenBy { it.name.lowercase() }
            .thenBy { it.brand.orEmpty().lowercase() },
    )
}

private fun String.urlDecode(): String = URLDecoder.decode(this, Charsets.UTF_8.name())
