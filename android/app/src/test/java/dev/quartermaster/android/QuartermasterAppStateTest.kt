package dev.quartermaster.android

import dev.quartermaster.android.generated.models.BarcodeLookupResponse
import dev.quartermaster.android.generated.models.CreateInviteRequest
import dev.quartermaster.android.generated.models.CreateStockRequest
import dev.quartermaster.android.generated.models.HouseholdDetailDto
import dev.quartermaster.android.generated.models.InviteDto
import dev.quartermaster.android.generated.models.LocationDto
import dev.quartermaster.android.generated.models.MeResponse
import dev.quartermaster.android.generated.models.ProductDto
import dev.quartermaster.android.generated.models.PushAuthorizationStatus
import dev.quartermaster.android.generated.models.ReminderDto
import dev.quartermaster.android.generated.models.StockBatchDto
import dev.quartermaster.android.generated.models.StockEventDto
import dev.quartermaster.android.generated.models.UnitDto
import dev.quartermaster.android.generated.infrastructure.Serializer
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class QuartermasterAppStateTest {
    private val json = Serializer.kotlinxSerializationJson

    @Test
    fun `parseInviteContext accepts custom join scheme and trims values`() {
        val context = QuartermasterAppState.parseInviteContext(
            "quartermaster://join?invite=%20ABCD1234%20&server=http%3A%2F%2F10.0.2.2%3A8080%2F"
        )

        assertEquals("ABCD1234", context?.inviteCode)
        assertEquals("http://10.0.2.2:8080", context?.serverUrl)
    }

    @Test
    fun `parseInviteContext accepts browser join link`() {
        val context = QuartermasterAppState.parseInviteContext(
            "https://quartermaster.example.com/join?invite=ZXCV9876&server=https%3A%2F%2Fexample.com"
        )

        assertEquals("ZXCV9876", context?.inviteCode)
        assertEquals("https://example.com", context?.serverUrl)
    }

    @Test
    fun `parseInviteContext ignores unrelated urls`() {
        val context = QuartermasterAppState.parseInviteContext(
            "https://quartermaster.example.com/inventory"
        )

        assertNull(context)
    }

    @Test
    fun `logout clears household scoped state and enters unauthenticated phase`() = runTest {
        val appState = QuartermasterAppState(
            sessionStore = FakeSessionStore(),
            backend = FakeBackend(
                meResponse = meResponseJson(
                    currentHouseholdJson = householdJson("66666666-6666-6666-6666-666666666666", "Home"),
                    householdsJson = listOf(householdJson("66666666-6666-6666-6666-666666666666", "Home")),
                ),
                stock = listOf(stockBatchJson()),
                reminders = listOf(reminderJson()),
                locations = listOf(locationJson()),
            ),
        )

        appState.bootstrap()
        appState.logout()

        assertEquals(AppPhase.Unauthenticated, appState.phase)
        assertEquals(emptyList<StockBatchDto>(), appState.batches)
        assertEquals(emptyList<ReminderDto>(), appState.reminders)
        assertNull(appState.pendingInventoryTarget)
    }

    @Test
    fun `unit helpers prefer product unit within the product family`() = runTest {
        val appState = QuartermasterAppState(
            sessionStore = FakeSessionStore(),
            backend = FakeBackend(
                meResponse = meResponseJson(
                    currentHouseholdJson = householdJson("66666666-6666-6666-6666-666666666666", "Home"),
                    householdsJson = listOf(householdJson("66666666-6666-6666-6666-666666666666", "Home")),
                ),
                stock = listOf(stockBatchJson()),
                locations = listOf(locationJson()),
                units = listOf(unitJson("kg", "mass"), unitJson("g", "mass"), unitJson("ml", "volume")),
            ),
        )

        appState.bootstrap()

        val product = appState.batches.first().product
        assertEquals(listOf("g", "kg"), appState.unitSymbolsFor(product))
        assertEquals("g", appState.defaultUnitSymbolFor(product))
    }

    @Test
    fun `openReminder keeps the inventory target after refreshing`() = runTest {
        val reminder = reminderJson()
        val appState = QuartermasterAppState(
            sessionStore = FakeSessionStore(),
            backend = FakeBackend(
                meResponse = meResponseJson(
                    currentHouseholdJson = householdJson("66666666-6666-6666-6666-666666666666", "Home"),
                    householdsJson = listOf(householdJson("66666666-6666-6666-6666-666666666666", "Home")),
                ),
                stock = listOf(stockBatchJson()),
                reminders = listOf(reminder),
                locations = listOf(locationJson()),
            ),
        )

        appState.bootstrap()
        appState.openReminder(reminder)

        assertEquals(MainTab.Inventory, appState.selectedTab)
        assertEquals("44444444-4444-4444-4444-444444444444", appState.pendingInventoryTarget?.productId)
        assertEquals("22222222-2222-2222-2222-222222222222", appState.pendingInventoryTarget?.locationId)
        assertEquals("33333333-3333-3333-3333-333333333333", appState.pendingInventoryTarget?.batchId)
    }

    @Test
    fun `scan action failure clears in-flight state and stores local error`() = runTest {
        val appState = QuartermasterAppState(
            sessionStore = FakeSessionStore(),
            backend = FakeBackend(
                meResponse = meResponseJson(
                    currentHouseholdJson = householdJson("66666666-6666-6666-6666-666666666666", "Home"),
                    householdsJson = listOf(householdJson("66666666-6666-6666-6666-666666666666", "Home")),
                ),
                barcodeFailure = ApiFailure(502, "barcode_lookup_failed", "Barcode lookup failed"),
            ),
        )

        appState.lookupBarcode("123")

        assertNull(appState.scanActionInFlight)
        assertEquals("Barcode lookup failed", appState.scanError)
    }

    @Test
    fun `authenticated invite deep link opens settings with pending invite context`() = runTest {
        val appState = QuartermasterAppState(
            sessionStore = FakeSessionStore(),
            backend = FakeBackend(
                meResponse = meResponseJson(
                    currentHouseholdJson = householdJson("66666666-6666-6666-6666-666666666666", "Home"),
                    householdsJson = listOf(householdJson("66666666-6666-6666-6666-666666666666", "Home")),
                ),
            ),
        )

        appState.bootstrap()
        appState.handleDeepLink("quartermaster://join?invite=DEEP1234")

        assertEquals(MainTab.Settings, appState.selectedTab)
        assertEquals("DEEP1234", appState.pendingInviteContext?.inviteCode)
    }

    private fun meResponseJson(
        currentHouseholdJson: String?,
        householdsJson: List<String>,
    ): MeResponse {
        val currentHousehold = currentHouseholdJson ?: "null"
        val households = householdsJson.joinToString(prefix = "[", postfix = "]")
        return json.decodeFromString(
            """
            {
              "user": {
                "id": "11111111-1111-1111-1111-111111111111",
                "username": "alice",
                "email": "alice@example.com"
              },
              "current_household": $currentHousehold,
              "households": $households,
              "public_base_url": "https://quartermaster.example.com"
            }
            """.trimIndent()
        )
    }

    private fun householdJson(id: String, name: String): String = """
        {
          "id": "$id",
          "name": "$name",
          "timezone": "UTC",
          "role": "admin",
          "joined_at": "2026-04-22T12:00:00Z"
        }
    """.trimIndent()

    private fun locationJson(): LocationDto = json.decodeFromString(
        """
        {
          "id": "22222222-2222-2222-2222-222222222222",
          "name": "Pantry",
          "kind": "pantry",
          "sort_order": 0
        }
        """.trimIndent()
    )

    private fun unitJson(code: String, family: String): UnitDto = json.decodeFromString(
        """
        {
          "code": "$code",
          "family": "$family",
          "to_base_milli": 1000
        }
        """.trimIndent()
    )

    private fun stockBatchJson(): StockBatchDto = json.decodeFromString(
        """
        {
          "id": "33333333-3333-3333-3333-333333333333",
          "product": {
            "id": "44444444-4444-4444-4444-444444444444",
            "name": "Flour",
            "family": "mass",
            "preferred_unit": "g",
            "source": "manual"
          },
          "location_id": "22222222-2222-2222-2222-222222222222",
          "initial_quantity": "1000",
          "quantity": "900",
          "unit": "g",
          "created_at": "2026-04-22T12:00:00Z"
        }
        """.trimIndent()
    )

    private fun reminderJson(): ReminderDto = json.decodeFromString(
        """
        {
          "id": "55555555-5555-5555-5555-555555555555",
          "kind": "expiry",
          "title": "Use flour soon",
          "body": "Pantry flour expires tomorrow.",
          "fire_at": "2026-04-23T09:00:00Z",
          "household_timezone": "UTC",
          "household_fire_local_at": "2026-04-23T09:00:00",
          "batch_id": "33333333-3333-3333-3333-333333333333",
          "product_id": "44444444-4444-4444-4444-444444444444",
          "location_id": "22222222-2222-2222-2222-222222222222"
        }
        """.trimIndent()
    )

    private class FakeSessionStore : SessionStore {
        private var snapshot = SessionSnapshot(
            serverUrl = "http://10.0.2.2:8080",
            accessToken = "access",
            refreshToken = "refresh",
        )

        override fun snapshot(): SessionSnapshot = snapshot

        override fun saveServerUrl(url: String) {
            snapshot = snapshot.copy(serverUrl = url)
        }

        override fun saveTokens(accessToken: String, refreshToken: String) {
            snapshot = snapshot.copy(accessToken = accessToken, refreshToken = refreshToken)
        }

        override fun clearTokens() {
            snapshot = snapshot.copy(accessToken = null, refreshToken = null)
        }

        override fun stableDeviceId(): String = "android-device-1"
    }

    private class FakeBackend(
        private val meResponse: MeResponse,
        private val stock: List<StockBatchDto> = emptyList(),
        private val reminders: List<ReminderDto> = emptyList(),
        private val locations: List<LocationDto> = emptyList(),
        private val units: List<UnitDto> = emptyList(),
        private val barcodeFailure: Throwable? = null,
    ) : QuartermasterBackend {
        override var serverUrl: String = "http://10.0.2.2:8080"

        override suspend fun me(): MeResponse = meResponse
        override suspend fun login(username: String, password: String) = Unit
        override suspend fun register(username: String, password: String, email: String?, inviteCode: String?) = Unit
        override suspend fun logout() = Unit
        override suspend fun switchHousehold(householdId: String): MeResponse = meResponse
        override suspend fun createHousehold(name: String, timezone: String): MeResponse = meResponse
        override suspend fun redeemInvite(inviteCode: String) = Unit
        override suspend fun currentHousehold(): HouseholdDetailDto {
            error("Unused in test")
        }

        override suspend fun householdInvites(): List<InviteDto> = emptyList()
        override suspend fun createInvite(body: CreateInviteRequest): InviteDto {
            error("Unused in test")
        }

        override suspend fun locations(): List<LocationDto> = locations
        override suspend fun units(): List<UnitDto> = units
        override suspend fun listStock(): List<StockBatchDto> = stock
        override suspend fun listEvents(limit: Int): List<StockEventDto> = emptyList()
        override suspend fun listReminders(limit: Int): List<ReminderDto> = reminders
        override suspend fun acknowledgeReminder(id: String) = Unit
        override suspend fun presentReminder(id: String) = Unit
        override suspend fun openReminder(id: String) = Unit

        override suspend fun registerDevice(
            deviceId: String,
            pushToken: String?,
            authorization: PushAuthorizationStatus,
            appVersion: String,
        ) = Unit

        override suspend fun searchProducts(query: String): List<ProductDto> = emptyList()

        override suspend fun lookupBarcode(barcode: String): BarcodeLookupResponse {
            barcodeFailure?.let { throw it }
            error("Unused in test")
        }

        override suspend fun addStock(request: CreateStockRequest): StockBatchDto {
            error("Unused in test")
        }
    }
}
