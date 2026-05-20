package dev.quartermaster.android

import android.util.Base64
import dev.quartermaster.android.generated.infrastructure.Serializer
import dev.quartermaster.android.generated.models.BarcodeLookupResponse
import dev.quartermaster.android.generated.models.ConfirmEmailVerificationRequest
import dev.quartermaster.android.generated.models.ConsumeRequest
import dev.quartermaster.android.generated.models.ConsumeResponse
import dev.quartermaster.android.generated.models.CreateHouseholdRequest
import dev.quartermaster.android.generated.models.CreateInviteRequest
import dev.quartermaster.android.generated.models.CreateLocationRequest
import dev.quartermaster.android.generated.models.CreateOnboardingHouseholdRequest
import dev.quartermaster.android.generated.models.CreateProductRequest
import dev.quartermaster.android.generated.models.CreateStockRequest
import dev.quartermaster.android.generated.models.CreateStorageVesselRequest
import dev.quartermaster.android.generated.models.HouseholdDetailDto
import dev.quartermaster.android.generated.models.InviteDto
import dev.quartermaster.android.generated.models.JoinInviteRequest
import dev.quartermaster.android.generated.models.LabelPrintStatus
import dev.quartermaster.android.generated.models.LocationDto
import dev.quartermaster.android.generated.models.LoginRequest
import dev.quartermaster.android.generated.models.MeResponse
import dev.quartermaster.android.generated.models.MeasurementSystem
import dev.quartermaster.android.generated.models.MemberDto
import dev.quartermaster.android.generated.models.OffContributionPreviewResponse
import dev.quartermaster.android.generated.models.OffContributionResponse
import dev.quartermaster.android.generated.models.OnboardingStatusResponse
import dev.quartermaster.android.generated.models.OpenFoodFactsCredentialStatusResponse
import dev.quartermaster.android.generated.models.PasswordResetConfirmRequest
import dev.quartermaster.android.generated.models.PasswordResetRequest
import dev.quartermaster.android.generated.models.PrintStockLabelRequest
import dev.quartermaster.android.generated.models.PrintStockLabelResponse
import dev.quartermaster.android.generated.models.ProductDto
import dev.quartermaster.android.generated.models.ProductSearchResponse
import dev.quartermaster.android.generated.models.PushAuthorizationStatus
import dev.quartermaster.android.generated.models.RedeemInviteRequest
import dev.quartermaster.android.generated.models.RefreshRequest
import dev.quartermaster.android.generated.models.RegisterDeviceRequest
import dev.quartermaster.android.generated.models.RegisterRequest
import dev.quartermaster.android.generated.models.ReminderDto
import dev.quartermaster.android.generated.models.ReminderListResponse
import dev.quartermaster.android.generated.models.RenderLabelResponse
import dev.quartermaster.android.generated.models.RequestEmailVerificationRequest
import dev.quartermaster.android.generated.models.RequestEmailVerificationResponse
import dev.quartermaster.android.generated.models.SaveOpenFoodFactsCredentialsRequest
import dev.quartermaster.android.generated.models.SplitStockRequest
import dev.quartermaster.android.generated.models.SplitStockResponse
import dev.quartermaster.android.generated.models.StockBatchDto
import dev.quartermaster.android.generated.models.StockEventDto
import dev.quartermaster.android.generated.models.StockEventListResponse
import dev.quartermaster.android.generated.models.StockListResponse
import dev.quartermaster.android.generated.models.StorageVesselDto
import dev.quartermaster.android.generated.models.SwitchHouseholdRequest
import dev.quartermaster.android.generated.models.TokenPair
import dev.quartermaster.android.generated.models.UnitDto
import dev.quartermaster.android.generated.models.UpdateHouseholdRequest
import dev.quartermaster.android.generated.models.UpdateLocationRequest
import dev.quartermaster.android.generated.models.UpdateStorageVesselRequest
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import kotlinx.coroutines.withContext
import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable
import kotlinx.serialization.encodeToString
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonPrimitive
import okhttp3.MediaType.Companion.toMediaType
import okhttp3.OkHttpClient
import okhttp3.Request
import okhttp3.RequestBody.Companion.toRequestBody
import okhttp3.logging.HttpLoggingInterceptor
import java.io.IOException
import java.util.UUID

class ApiFailure(
    val status: Int,
    val code: String?,
    override val message: String,
) : IOException(message)

@Serializable
data class ProductUpdateRequest(
    val operations: List<JsonPatchOperation>,
)

@Serializable
data class StockUpdateRequest(
    val operations: List<JsonPatchOperation>,
)

@Serializable
data class DeleteHouseholdRequest(
    @SerialName("confirmation_name")
    val confirmationName: String,
)

@Serializable
data class PasskeyCredentialSummary(
    val id: String,
    val label: String? = null,
    @SerialName("created_at")
    val createdAt: String,
    @SerialName("last_used_at")
    val lastUsedAt: String? = null,
)

@Serializable
data class PasskeyListResponse(
    val credentials: List<PasskeyCredentialSummary>,
)

@Serializable
data class PasskeyRegistrationStartRequest(
    val label: String? = null,
)

@Serializable
data class PasskeyRegistrationStartResponse(
    @SerialName("ceremony_id")
    val ceremonyId: String,
    @SerialName("public_key")
    val publicKey: JsonElement,
)

@Serializable
data class PasskeyRegistrationFinishRequest(
    @SerialName("ceremony_id")
    val ceremonyId: String,
    val credential: JsonElement,
    val label: String? = null,
)

@Serializable
data class PasskeyLoginStartRequest(
    val email: String,
)

@Serializable
data class PasskeyLoginStartResponse(
    @SerialName("ceremony_id")
    val ceremonyId: String,
    @SerialName("public_key")
    val publicKey: JsonElement,
)

@Serializable
data class PasskeyLoginFinishRequest(
    @SerialName("ceremony_id")
    val ceremonyId: String,
    val credential: JsonElement,
    @SerialName("device_label")
    val deviceLabel: String? = null,
)

@Serializable
data class AuthHandoffCreateRequest(
    @SerialName("target_device_label")
    val targetDeviceLabel: String? = null,
    @SerialName("server_url")
    val serverUrl: String? = null,
)

@Serializable
data class AuthHandoffCreateResponse(
    val id: String,
    @SerialName("handoff_url")
    val handoffUrl: String,
    @SerialName("expires_at")
    val expiresAt: String,
    @SerialName("target_device_label")
    val targetDeviceLabel: String? = null,
)

@Serializable
data class AuthHandoffTokenRequest(
    val id: String,
    val token: String,
)

@Serializable
data class AuthHandoffPreviewResponse(
    val id: String,
    @SerialName("source_email")
    val sourceEmail: String,
    @SerialName("source_display_name")
    val sourceDisplayName: String,
    @SerialName("household_id")
    val householdId: String? = null,
    @SerialName("target_device_label")
    val targetDeviceLabel: String? = null,
    @SerialName("expires_at")
    val expiresAt: String,
)

@Serializable
data class AuthHandoffAcceptRequest(
    val id: String,
    val token: String,
    @SerialName("device_label")
    val deviceLabel: String? = null,
)

@Serializable
data class JsonPatchOperation(
    val op: String,
    val path: String,
    val value: JsonElement? = null,
) {
    constructor(op: String, path: String, value: String) : this(op, path, JsonPrimitive(value))
    constructor(op: String, path: String, value: Long) : this(op, path, JsonPrimitive(value))
}

class QuartermasterApi(
    private val authStore: AuthStore,
    private val labelPrinterSender: LabelPrinterSender = TcpLabelPrinterSender(),
) {
    private val apiPrefix = "/api/v1"
    private val json = Serializer.kotlinxSerializationJson
    private val patchJson = Json(json) { encodeDefaults = false }
    private val refreshMutex = Mutex()
    private val client =
        OkHttpClient
            .Builder()
            .addInterceptor(
                HttpLoggingInterceptor().apply {
                    level = HttpLoggingInterceptor.Level.BASIC
                },
            ).build()

    var serverUrl: String
        get() = authStore.snapshot().serverUrl
        set(value) = authStore.saveServerUrl(value)

    suspend fun me(): MeResponse = authedJson("GET", "/auth/me")

    suspend fun onboardingStatus(): OnboardingStatusResponse = jsonRequest(
        method = "GET",
        path = "/onboarding/status",
        body = null,
        requiresAuth = false,
    )

    suspend fun createOnboardingHousehold(
        email: String,
        displayName: String,
        password: String,
        householdName: String,
        timezone: String,
        deviceLabel: String = "Android",
    ): TokenPair = jsonRequest<TokenPair>(
        method = "POST",
        path = "/onboarding/create-household",
        body =
        CreateOnboardingHouseholdRequest(
            email = email,
            displayName = displayName,
            password = password,
            householdName = householdName,
            timezone = timezone,
            deviceLabel = deviceLabel,
        ),
        requiresAuth = false,
    ).also { authStore.saveTokens(it.accessToken, it.refreshToken) }

    suspend fun joinOnboardingInvite(
        email: String,
        displayName: String,
        password: String,
        inviteCode: String,
        deviceLabel: String = "Android",
    ): TokenPair = jsonRequest<TokenPair>(
        method = "POST",
        path = "/onboarding/join-invite",
        body =
        JoinInviteRequest(
            email = email,
            displayName = displayName,
            password = password,
            inviteCode = inviteCode,
            deviceLabel = deviceLabel,
        ),
        requiresAuth = false,
    ).also { authStore.saveTokens(it.accessToken, it.refreshToken) }

    suspend fun login(
        email: String,
        password: String,
        deviceLabel: String = "Android",
    ): TokenPair = jsonRequest<TokenPair>(
        method = "POST",
        path = "/auth/login",
        body =
        LoginRequest(
            email = email,
            password = password,
            deviceLabel = deviceLabel,
        ),
        requiresAuth = false,
    ).also { authStore.saveTokens(it.accessToken, it.refreshToken) }

    suspend fun listPasskeys(): List<PasskeyCredentialSummary> = authedJson<PasskeyListResponse>(
        method = "GET",
        path = "/auth/passkeys",
        body = null,
    ).credentials

    suspend fun startPasskeyRegistration(label: String?): PasskeyRegistrationStartResponse = authedJson(
        method = "POST",
        path = "/auth/passkeys/register/start",
        body = PasskeyRegistrationStartRequest(label = label),
    )

    suspend fun finishPasskeyRegistration(
        ceremonyId: String,
        credential: JsonElement,
        label: String?,
    ): PasskeyCredentialSummary = authedJson(
        method = "POST",
        path = "/auth/passkeys/register/finish",
        body = PasskeyRegistrationFinishRequest(ceremonyId = ceremonyId, credential = credential, label = label),
    )

    suspend fun startPasskeyLogin(email: String): PasskeyLoginStartResponse = jsonRequest(
        method = "POST",
        path = "/auth/passkeys/login/start",
        body = PasskeyLoginStartRequest(email = email),
        requiresAuth = false,
    )

    suspend fun finishPasskeyLogin(ceremonyId: String, credential: JsonElement): TokenPair = jsonRequest<TokenPair>(
        method = "POST",
        path = "/auth/passkeys/login/finish",
        body = PasskeyLoginFinishRequest(ceremonyId = ceremonyId, credential = credential, deviceLabel = "Android"),
        requiresAuth = false,
    ).also { authStore.saveTokens(it.accessToken, it.refreshToken) }

    suspend fun deletePasskey(id: String) {
        authedUnit("DELETE", "/auth/passkeys/$id")
    }

    suspend fun createAuthHandoff(targetDeviceLabel: String?, serverUrl: String?): AuthHandoffCreateResponse = authedJson(
        method = "POST",
        path = "/auth/handoffs",
        body = AuthHandoffCreateRequest(targetDeviceLabel = targetDeviceLabel, serverUrl = serverUrl),
    )

    suspend fun cancelAuthHandoff(id: String) {
        authedUnit("DELETE", "/auth/handoffs/$id")
    }

    suspend fun previewAuthHandoff(id: String, token: String): AuthHandoffPreviewResponse = jsonRequest(
        method = "POST",
        path = "/auth/handoffs/preview",
        body = AuthHandoffTokenRequest(id = id, token = token),
        requiresAuth = false,
    )

    suspend fun acceptAuthHandoff(id: String, token: String, deviceLabel: String? = "Android"): TokenPair = jsonRequest<TokenPair>(
        method = "POST",
        path = "/auth/handoffs/accept",
        body = AuthHandoffAcceptRequest(id = id, token = token, deviceLabel = deviceLabel),
        requiresAuth = false,
    ).also { authStore.saveTokens(it.accessToken, it.refreshToken) }

    suspend fun register(
        email: String,
        displayName: String,
        password: String,
        inviteCode: String?,
        deviceLabel: String = "Android",
    ): TokenPair = jsonRequest<TokenPair>(
        method = "POST",
        path = "/auth/register",
        body =
        RegisterRequest(
            email = email,
            displayName = displayName,
            password = password,
            inviteCode = inviteCode,
            deviceLabel = deviceLabel,
        ),
        requiresAuth = false,
    ).also { authStore.saveTokens(it.accessToken, it.refreshToken) }

    suspend fun requestEmailVerification(email: String): RequestEmailVerificationResponse = authedJson(
        method = "POST",
        path = "/auth/email-verification",
        body = RequestEmailVerificationRequest(email = email),
    )

    suspend fun confirmEmailVerification(code: String): MeResponse = authedJson(
        method = "POST",
        path = "/auth/email-verification/confirm",
        body = ConfirmEmailVerificationRequest(code = code),
    )

    suspend fun clearRecoveryEmail(): MeResponse = authedJson(
        method = "DELETE",
        path = "/auth/email",
        body = null,
    )

    suspend fun requestPasswordReset(email: String) {
        unitRequest(
            method = "POST",
            path = "/auth/password-reset/request",
            body = PasswordResetRequest(email = email),
            requiresAuth = false,
        )
    }

    suspend fun confirmPasswordReset(email: String, newPassword: String, code: String) {
        unitRequest(
            method = "POST",
            path = "/auth/password-reset/confirm",
            body = PasswordResetConfirmRequest(
                email = email,
                newPassword = newPassword,
                code = code,
                token = null,
            ),
            requiresAuth = false,
        )
    }

    suspend fun logout() {
        runCatching { authedUnit("POST", "/auth/logout") }
        authStore.clearTokens()
    }

    suspend fun switchHousehold(householdId: String): MeResponse = authedJson(
        method = "POST",
        path = "/auth/switch-household",
        body = SwitchHouseholdRequest(householdId = UUID.fromString(householdId)),
    )

    suspend fun createHousehold(
        name: String,
        timezone: String,
        measurementSystem: MeasurementSystem = MeasurementSystem.METRIC,
    ): MeResponse = authedJson(
        method = "POST",
        path = "/households",
        body = CreateHouseholdRequest(
            name = name,
            timezone = timezone,
            measurementSystem = measurementSystem,
        ),
    )

    suspend fun redeemInvite(inviteCode: String) {
        authedUnit(
            method = "POST",
            path = "/invites/redeem",
            body = RedeemInviteRequest(inviteCode = inviteCode),
        )
    }

    suspend fun currentHousehold(): HouseholdDetailDto = authedJson("GET", "/households/current")

    suspend fun updateCurrentHousehold(request: UpdateHouseholdRequest): HouseholdDetailDto = authedJson(
        method = "PATCH",
        path = "/households/current",
        body = request,
    )

    suspend fun exportCurrentHousehold(): String = authedRaw("GET", "/households/current/export")

    suspend fun importHousehold(document: JsonElement): MeResponse = authedJson(
        method = "POST",
        path = "/households/import",
        body = document,
    )

    suspend fun requestCurrentHouseholdDeletion(confirmationName: String) {
        authedUnit(
            method = "POST",
            path = "/households/current/deletion",
            body = DeleteHouseholdRequest(confirmationName = confirmationName),
        )
    }

    suspend fun householdMembers(): List<MemberDto> = authedJson("GET", "/households/current/members")

    suspend fun householdInvites(): List<InviteDto> = authedJson("GET", "/households/current/invites")

    suspend fun createInvite(body: CreateInviteRequest): InviteDto = authedJson(
        method = "POST",
        path = "/households/current/invites",
        body = body,
    )

    suspend fun locations(): List<LocationDto> = authedJson("GET", "/locations")

    suspend fun createLocation(request: CreateLocationRequest): LocationDto = authedJson(
        method = "POST",
        path = "/locations",
        body = request,
    )

    suspend fun updateLocation(
        id: String,
        request: UpdateLocationRequest,
    ): LocationDto = authedJson(
        method = "PATCH",
        path = "/locations/${id.urlEncode()}",
        body = request,
    )

    suspend fun deleteLocation(id: String) {
        authedUnit("DELETE", "/locations/${id.urlEncode()}")
    }

    suspend fun storageVessels(): List<StorageVesselDto> = authedJson("GET", "/storage-vessels")

    suspend fun createStorageVessel(request: CreateStorageVesselRequest): StorageVesselDto = authedJson(
        method = "POST",
        path = "/storage-vessels",
        body = request,
    )

    suspend fun updateStorageVessel(
        id: String,
        request: UpdateStorageVesselRequest,
    ): StorageVesselDto = authedJson(
        method = "PATCH",
        path = "/storage-vessels/${id.urlEncode()}",
        body = request,
    )

    suspend fun deleteStorageVessel(id: String) {
        authedUnit("DELETE", "/storage-vessels/${id.urlEncode()}")
    }

    suspend fun units(): List<UnitDto> = authedJson("GET", "/units")

    suspend fun listStock(includeDepleted: Boolean = false): List<StockBatchDto> {
        val query = if (includeDepleted) "?include_depleted=true" else ""
        return authedJson<StockListResponse>("GET", "/stock$query").items
    }

    suspend fun getStock(id: String): StockBatchDto = authedJson(
        method = "GET",
        path = "/stock/${id.urlEncode()}",
    )

    suspend fun listEvents(limit: Int = 30): List<StockEventDto> = authedJson<StockEventListResponse>("GET", "/stock/events?limit=$limit").items

    suspend fun listBatchEvents(
        batchId: String,
        limit: Int = 30,
    ): List<StockEventDto> = authedJson<StockEventListResponse>("GET", "/stock/${batchId.urlEncode()}/events?limit=$limit").items

    suspend fun listReminders(limit: Int = 50): List<ReminderDto> = authedJson<ReminderListResponse>("GET", "/reminders?limit=$limit").items

    suspend fun acknowledgeReminder(id: String) {
        authedUnit("POST", "/reminders/$id/ack")
    }

    suspend fun presentReminder(id: String) {
        authedUnit("POST", "/reminders/$id/present")
    }

    suspend fun openReminder(id: String) {
        authedUnit("POST", "/reminders/$id/open")
    }

    suspend fun registerDevice(
        deviceId: String,
        pushToken: String?,
        authorization: PushAuthorizationStatus,
        appVersion: String,
    ) {
        authedUnit(
            method = "POST",
            path = "/devices/register",
            body =
            RegisterDeviceRequest(
                deviceId = deviceId,
                platform = "android",
                pushToken = pushToken,
                pushAuthorization = authorization,
                appVersion = appVersion,
            ),
        )
    }

    suspend fun searchProducts(query: String): List<ProductDto> = authedJson<ProductSearchResponse>(
        "GET",
        "/products/search?query=${query.urlEncode()}&limit=20",
    ).items

    suspend fun listProducts(
        query: String?,
        limit: Int = 100,
        includeDeleted: Boolean = false,
    ): List<ProductDto> {
        val params = mutableListOf("limit=$limit", "include_deleted=$includeDeleted")
        query?.trim()?.takeIf(String::isNotEmpty)?.let { params += "q=${it.urlEncode()}" }
        return authedJson<ProductSearchResponse>(
            "GET",
            "/products?${params.joinToString("&")}",
        ).items
    }

    suspend fun getProduct(id: String): ProductDto = authedJson(
        method = "GET",
        path = "/products/${id.urlEncode()}",
    )

    suspend fun createProduct(request: CreateProductRequest): ProductDto = authedJson(
        method = "POST",
        path = "/products",
        body = request,
    )

    suspend fun updateProduct(
        id: String,
        request: ProductUpdateRequest,
    ): ProductDto = authedJson(
        method = "PATCH",
        path = "/products/${id.urlEncode()}",
        body = request,
    )

    suspend fun deleteProduct(id: String) {
        authedUnit("DELETE", "/products/${id.urlEncode()}")
    }

    suspend fun restoreProduct(id: String): ProductDto = authedJson(
        method = "POST",
        path = "/products/${id.urlEncode()}/restore",
    )

    suspend fun refreshProduct(id: String): ProductDto = authedJson(
        method = "POST",
        path = "/products/${id.urlEncode()}/refresh",
    )

    suspend fun offContributionPreview(id: String): OffContributionPreviewResponse = authedJson(
        method = "GET",
        path = "/products/${id.urlEncode()}/off-contribution-preview",
    )

    suspend fun contributeProductToOff(id: String): OffContributionResponse = authedJson(
        method = "POST",
        path = "/products/${id.urlEncode()}/off-contribution",
    )

    suspend fun openFoodFactsCredentialStatus(): OpenFoodFactsCredentialStatusResponse = authedJson(
        method = "GET",
        path = "/account/openfoodfacts/status",
    )

    suspend fun saveOpenFoodFactsCredentials(
        request: SaveOpenFoodFactsCredentialsRequest,
    ): OpenFoodFactsCredentialStatusResponse = authedJson(
        method = "PUT",
        path = "/account/openfoodfacts",
        body = request,
    )

    suspend fun deleteOpenFoodFactsCredentials() {
        authedUnit("DELETE", "/account/openfoodfacts")
    }

    suspend fun lookupBarcode(barcode: String): BarcodeLookupResponse = authedJson("GET", "/products/by-barcode/${barcode.urlEncode()}")

    suspend fun addStock(request: CreateStockRequest): StockBatchDto = authedJson(
        method = "POST",
        path = "/stock",
        body = request,
    )

    suspend fun updateStock(
        id: String,
        request: StockUpdateRequest,
    ): StockBatchDto = authedJson(
        method = "PATCH",
        path = "/stock/${id.urlEncode()}",
        body = request,
    )

    suspend fun consumeStock(request: ConsumeRequest): ConsumeResponse = authedJson(
        method = "POST",
        path = "/stock/consume",
        body = request,
    )

    suspend fun splitStock(
        batchId: String,
        request: SplitStockRequest,
    ): SplitStockResponse = authedJson(
        method = "POST",
        path = "/stock/${batchId.urlEncode()}/split",
        body = request,
    )

    suspend fun printStockLabel(
        batchId: String,
        request: PrintStockLabelRequest = PrintStockLabelRequest(
            copies = 1,
            dryRun = null,
            includeQuantity = false,
            labelSize = "standard",
            printerId = null,
        ),
    ): PrintStockLabelResponse = authedJson(
        method = "POST",
        path = "/stock/${batchId.urlEncode()}/labels/print",
        body = request,
    )

    suspend fun printStockLabel(batchId: String): PrintStockLabelResponse {
        val request = PrintStockLabelRequest()
        return try {
            authedJson(
                method = "POST",
                path = "/stock/${batchId.urlEncode()}/labels/print",
                body = request,
            )
        } catch (failure: ApiFailure) {
            if (!failure.message.contains("client delivery", ignoreCase = true)) {
                throw failure
            }
            val artifact: RenderLabelResponse = authedJson(
                method = "POST",
                path = "/stock/${batchId.urlEncode()}/labels/render",
                body = request,
            )
            val payload = Base64.decode(artifact.payload, Base64.DEFAULT)
            labelPrinterSender.send(payload, artifact.address, artifact.port.toInt())
            PrintStockLabelResponse(
                printerId = artifact.printerId,
                batchId = artifact.batchId,
                batchUrl = artifact.batchUrl,
                copies = artifact.copies,
                status = LabelPrintStatus.SENT,
            )
        }
    }

    suspend fun discardStock(batchId: String) {
        authedUnit("DELETE", "/stock/${batchId.urlEncode()}")
    }

    suspend fun restoreStock(batchId: String): StockBatchDto = authedJson(
        method = "POST",
        path = "/stock/${batchId.urlEncode()}/restore",
    )

    private suspend inline fun <reified T> authedJson(
        method: String,
        path: String,
        body: Any? = null,
    ): T = withAuthRetry {
        jsonRequest<T>(method = method, path = path, body = body, requiresAuth = true)
    }

    private suspend fun authedRaw(
        method: String,
        path: String,
        body: Any? = null,
    ): String = withAuthRetry {
        rawRequest(method = method, path = path, body = body, requiresAuth = true)
    }

    private suspend fun authedUnit(
        method: String,
        path: String,
        body: Any? = null,
    ) {
        withAuthRetry {
            unitRequest(method = method, path = path, body = body, requiresAuth = true)
        }
    }

    private suspend fun <T> withAuthRetry(block: suspend () -> T): T {
        try {
            return block()
        } catch (failure: ApiFailure) {
            if (failure.status != 401 || !refreshTokens()) throw failure
        }
        return block()
    }

    private suspend fun refreshTokens(): Boolean = refreshMutex.withLock {
        val refreshToken = authStore.snapshot().refreshToken ?: return false
        return try {
            val pair: TokenPair =
                jsonRequest<TokenPair>(
                    method = "POST",
                    path = "/auth/refresh",
                    body = RefreshRequest(refreshToken = refreshToken),
                    requiresAuth = false,
                )
            authStore.saveTokens(pair.accessToken, pair.refreshToken)
            true
        } catch (_: Exception) {
            authStore.clearTokens()
            false
        }
    }

    private suspend inline fun <reified T> jsonRequest(
        method: String,
        path: String,
        body: Any? = null,
        requiresAuth: Boolean,
    ): T = withContext(Dispatchers.IO) {
        val raw = execute(method, path, body, requiresAuth)
        if (raw.isBlank()) {
            throw ApiFailure(500, null, "Expected a JSON response")
        }
        json.decodeFromString<T>(raw)
    }

    private suspend fun rawRequest(
        method: String,
        path: String,
        body: Any? = null,
        requiresAuth: Boolean,
    ): String = withContext(Dispatchers.IO) {
        execute(method, path, body, requiresAuth)
    }

    private suspend fun unitRequest(
        method: String,
        path: String,
        body: Any? = null,
        requiresAuth: Boolean,
    ) {
        withContext(Dispatchers.IO) {
            execute(method, path, body, requiresAuth)
        }
    }

    private fun execute(
        method: String,
        path: String,
        body: Any?,
        requiresAuth: Boolean,
    ): String {
        val snapshot = authStore.snapshot()
        val requestBuilder =
            Request
                .Builder()
                .url(snapshot.serverUrl.removeSuffix("/") + apiPrefix + path)
                .header("Accept", "application/json")

        if (requiresAuth) {
            val token = snapshot.accessToken ?: throw ApiFailure(401, null, "Not signed in")
            requestBuilder.header("Authorization", "Bearer $token")
        }

        val requestBody =
            body?.let {
                encodeBody(it)
                    .toRequestBody(JSON_MEDIA_TYPE)
            }

        val request =
            when (method) {
                "GET" -> requestBuilder.get().build()
                "POST" -> requestBuilder.post(requestBody ?: EMPTY_JSON_BODY).build()
                "PATCH" -> requestBuilder.patch(requestBody ?: EMPTY_JSON_BODY).build()
                "DELETE" -> requestBuilder.delete(requestBody).build()
                else -> error("Unsupported method $method")
            }

        client.newCall(request).execute().use { response ->
            val payload = response.body?.string().orEmpty()
            if (!response.isSuccessful) {
                val error =
                    payload.takeIf(String::isNotBlank)?.let {
                        runCatching { json.decodeFromString<ApiErrorBody>(it) }.getOrNull()
                    }
                throw ApiFailure(
                    status = response.code,
                    code = error?.code,
                    message = error?.message ?: "Request failed with ${response.code}",
                )
            }
            return payload
        }
    }

    @Serializable
    private data class ApiErrorBody(
        val code: String? = null,
        val message: String? = null,
    )

    companion object {
        private val JSON_MEDIA_TYPE = "application/json; charset=utf-8".toMediaType()
        private val EMPTY_JSON_BODY = "{}".toRequestBody(JSON_MEDIA_TYPE)
    }

    private fun encodeBody(body: Any): String = when (body) {
        is CreateHouseholdRequest -> json.encodeToString(body)
        is CreateInviteRequest -> json.encodeToString(body)
        is CreateLocationRequest -> json.encodeToString(body)
        is CreateProductRequest -> json.encodeToString(body)
        is CreateStockRequest -> json.encodeToString(body)
        is CreateStorageVesselRequest -> json.encodeToString(body)
        is DeleteHouseholdRequest -> json.encodeToString(body)
        is ConsumeRequest -> json.encodeToString(body)
        is JsonElement -> json.encodeToString(JsonElement.serializer(), body)
        is LoginRequest -> json.encodeToString(body)
        is RefreshRequest -> json.encodeToString(body)
        is RedeemInviteRequest -> json.encodeToString(body)
        is PasswordResetConfirmRequest -> json.encodeToString(body)
        is PasswordResetRequest -> json.encodeToString(body)
        is PrintStockLabelRequest -> json.encodeToString(body)
        is RegisterDeviceRequest -> json.encodeToString(body)
        is RegisterRequest -> json.encodeToString(body)
        is SwitchHouseholdRequest -> json.encodeToString(body)
        is UpdateHouseholdRequest -> json.encodeToString(body)
        is UpdateLocationRequest -> json.encodeToString(body)
        is UpdateStorageVesselRequest -> json.encodeToString(body)
        is ProductUpdateRequest -> patchJson.encodeToString(body.operations)
        is StockUpdateRequest -> patchJson.encodeToString(body.operations)
        else -> error("Unsupported request body type: ${body::class.qualifiedName}")
    }
}

private fun String.urlEncode(): String = java.net.URLEncoder.encode(this, Charsets.UTF_8.name())
