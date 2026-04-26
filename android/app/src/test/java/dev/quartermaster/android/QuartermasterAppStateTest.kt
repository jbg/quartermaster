package dev.quartermaster.android

import dev.quartermaster.android.generated.infrastructure.Serializer
import dev.quartermaster.android.generated.models.BarcodeLookupResponse
import dev.quartermaster.android.generated.models.ConsumeRequest
import dev.quartermaster.android.generated.models.ConsumeResponse
import dev.quartermaster.android.generated.models.ConsumedBatchDto
import dev.quartermaster.android.generated.models.CreateInviteRequest
import dev.quartermaster.android.generated.models.CreateLocationRequest
import dev.quartermaster.android.generated.models.CreateProductRequest
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
import dev.quartermaster.android.generated.models.StockEventType
import dev.quartermaster.android.generated.models.UnitDto
import dev.quartermaster.android.generated.models.UnitFamily
import dev.quartermaster.android.generated.models.UpdateLocationRequest
import kotlinx.coroutines.test.runTest
import kotlinx.serialization.encodeToString
import kotlinx.serialization.json.Json
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import java.util.UUID

class QuartermasterAppStateTest {
    private val json = Serializer.kotlinxSerializationJson

    @Test
    fun `reminder date formatting is readable with raw fallback`() {
        assertEquals("not-a-date", formatReminderDate("not-a-date"))
        assertEquals("not-a-date", formatReminderDateTime("not-a-date"))
        assertTrue(formatReminderDate("2026-04-24") != "2026-04-24")
        assertTrue(
            formatReminderDateTime("2026-04-23T09:00:00+02:00") !=
                "2026-04-23T09:00:00+02:00",
        )
    }

    @Test
    fun `parseInviteContext accepts custom join scheme and trims values`() {
        val context =
            QuartermasterAppState.parseInviteContext(
                "quartermaster://join?invite=%20ABCD1234%20&server=http%3A%2F%2F10.0.2.2%3A8080%2F",
            )

        assertEquals("ABCD1234", context?.inviteCode)
        assertEquals("http://10.0.2.2:8080", context?.serverUrl)
    }

    @Test
    fun `parseInviteContext accepts browser join link`() {
        val context =
            QuartermasterAppState.parseInviteContext(
                "https://quartermaster.example.com/join?invite=ZXCV9876&server=https%3A%2F%2Fexample.com",
            )

        assertEquals("ZXCV9876", context?.inviteCode)
        assertEquals("https://example.com", context?.serverUrl)
    }

    @Test
    fun `parseInviteContext ignores unrelated urls`() {
        val context =
            QuartermasterAppState.parseInviteContext(
                "https://quartermaster.example.com/inventory",
            )

        assertNull(context)
    }

    @Test
    fun `bootstrap restores household scoped data and marks sections loaded`() = runTest {
        val backend =
            FakeBackend(
                meResponse = meResponseJson(),
                stock = listOf(stockBatchJson()),
                reminders = listOf(reminderJson()),
                locations = listOf(locationJson()),
                units = listOf(unitJson("g", "mass")),
                householdDetail = householdDetailJson(),
            )
        val appState =
            QuartermasterAppState(
                sessionStore = FakeSessionStore(),
                backend = backend,
            )

        appState.bootstrap()

        assertTrue(appState.hasLoadedInventoryOnce)
        assertTrue(appState.hasLoadedRemindersOnce)
        assertTrue(appState.hasLoadedSettingsOnce)
        assertEquals(1, appState.batches.size)
        assertEquals(1, appState.reminders.size)
        assertEquals(1, appState.locations.size)
    }

    @Test
    fun `logout clears household scoped state invite handoff and enters unauthenticated phase`() = runTest {
        val appState =
            QuartermasterAppState(
                sessionStore = FakeSessionStore(),
                backend =
                FakeBackend(
                    meResponse = meResponseJson(),
                    stock = listOf(stockBatchJson()),
                    reminders = listOf(reminderJson()),
                    locations = listOf(locationJson()),
                    products = listOf(productDtoJson()),
                ),
            )

        appState.bootstrap()
        appState.handleDeepLink("quartermaster://join?invite=DEEP1234")
        appState.openReminder(reminderJson())
        appState.openProduct("44444444-4444-4444-4444-444444444444")
        appState.logout()

        assertEquals(AppPhase.Unauthenticated, appState.phase)
        assertEquals(emptyList<StockBatchDto>(), appState.batches)
        assertEquals(emptyList<ReminderDto>(), appState.reminders)
        assertEquals(emptyList<ProductDto>(), appState.products)
        assertNull(appState.selectedCatalogueProduct)
        assertNull(appState.pendingInventoryTarget)
        assertNull(appState.pendingInviteContext)
        assertFalse(appState.hasLoadedInventoryOnce)
        assertFalse(appState.hasLoadedProductsOnce)
        assertFalse(appState.hasLoadedRemindersOnce)
    }

    @Test
    fun `logout still clears session when backend logout throws`() = runTest {
        val appState =
            QuartermasterAppState(
                sessionStore = FakeSessionStore(),
                backend =
                FakeBackend(
                    meResponse = meResponseJson(),
                    logoutFailure = RuntimeException("boom"),
                ),
            )

        appState.bootstrap()
        appState.logout()

        assertEquals(AppPhase.Unauthenticated, appState.phase)
        assertNull(appState.pendingInviteContext)
        assertNull(appState.pendingInventoryTarget)
    }

    @Test
    fun `unit helpers prefer product unit within the product family`() = runTest {
        val appState =
            QuartermasterAppState(
                sessionStore = FakeSessionStore(),
                backend =
                FakeBackend(
                    meResponse = meResponseJson(),
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
        val appState =
            QuartermasterAppState(
                sessionStore = FakeSessionStore(),
                backend =
                FakeBackend(
                    meResponse = meResponseJson(),
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
        assertTrue(appState.hasLoadedInventoryOnce)
        assertTrue(appState.hasLoadedRemindersOnce)
    }

    @Test
    fun `acknowledgeReminder removes reminder and refreshes due list`() = runTest {
        val firstReminder = reminderJson()
        val secondReminder =
            reminderJson(
                id = "aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa",
                title = "Use beans soon",
            )
        val backend =
            FakeBackend(
                meResponse = meResponseJson(),
                reminders = listOf(firstReminder, secondReminder),
            )
        val appState =
            QuartermasterAppState(
                sessionStore = FakeSessionStore(),
                backend = backend,
            )

        appState.bootstrap()
        appState.acknowledgeReminder(firstReminder.id.toString())

        assertEquals(1, appState.reminders.size)
        assertEquals(secondReminder.id, appState.reminders.first().id)
        assertEquals(listOf(firstReminder.id.toString()), backend.acknowledgedReminderIds)
    }

    @Test
    fun `reminder action failure clears in flight state and stores local error`() = runTest {
        val reminder = reminderJson()
        val appState =
            QuartermasterAppState(
                sessionStore = FakeSessionStore(),
                backend =
                FakeBackend(
                    meResponse = meResponseJson(),
                    reminders = listOf(reminder),
                    openReminderFailure = ApiFailure(502, "open_reminder_failed", "Reminder open failed"),
                ),
            )

        appState.bootstrap()
        appState.openReminder(reminder)

        assertNull(appState.reminderActionFor(reminder.id.toString()))
        assertEquals("Reminder open failed", appState.lastError)
        assertEquals(1, appState.reminders.size)
    }

    @Test
    fun `scan action failure clears in flight state and stores local error`() = runTest {
        val appState =
            QuartermasterAppState(
                sessionStore = FakeSessionStore(),
                backend =
                FakeBackend(
                    meResponse = meResponseJson(),
                    barcodeFailure = ApiFailure(502, "barcode_lookup_failed", "Barcode lookup failed"),
                ),
            )

        appState.lookupBarcode("123")

        assertNull(appState.scanActionInFlight)
        assertEquals("Barcode lookup failed", appState.scanError)
    }

    @Test
    fun `authenticated invite deep link opens settings with pending invite context`() = runTest {
        val appState =
            QuartermasterAppState(
                sessionStore = FakeSessionStore(),
                backend = FakeBackend(meResponse = meResponseJson()),
            )

        appState.bootstrap()
        appState.handleDeepLink("quartermaster://join?invite=DEEP1234")

        assertEquals(MainTab.Settings, appState.selectedTab)
        assertEquals("DEEP1234", appState.pendingInviteContext?.inviteCode)
    }

    @Test
    fun `unauthenticated invite deep link stores pending invite context for onboarding`() = runTest {
        val appState =
            QuartermasterAppState(
                sessionStore = FakeSessionStore(accessToken = null, refreshToken = null),
                backend = FakeBackend(meResponse = meResponseJson()),
            )

        appState.bootstrap()
        appState.handleDeepLink("quartermaster://join?invite=DEEP1234&server=http%3A%2F%2F10.0.2.2%3A8080")

        assertEquals(AppPhase.Unauthenticated, appState.phase)
        assertEquals("DEEP1234", appState.pendingInviteContext?.inviteCode)
        assertEquals("http://10.0.2.2:8080", appState.pendingInviteContext?.serverUrl)
    }

    @Test
    fun `handleReminderPayload opens reminder and refreshes when authenticated`() = runTest {
        val backend =
            FakeBackend(
                meResponse = meResponseJson(),
                stock = listOf(stockBatchJson()),
                reminders = listOf(reminderJson()),
                locations = listOf(locationJson()),
            )
        val appState =
            QuartermasterAppState(
                sessionStore = FakeSessionStore(),
                backend = backend,
            )

        appState.bootstrap()
        appState.handleReminderPayload(reminderPayload())

        assertEquals(MainTab.Inventory, appState.selectedTab)
        assertEquals("55555555-5555-5555-5555-555555555555", backend.openedReminderIds.single())
        assertEquals("33333333-3333-3333-3333-333333333333", appState.pendingInventoryTarget?.batchId)
        assertTrue(appState.hasLoadedInventoryOnce)
        assertTrue(appState.hasLoadedRemindersOnce)
    }

    @Test
    fun `handleReminderPayload stores target without opening reminder when unauthenticated`() = runTest {
        val backend = FakeBackend(meResponse = meResponseJson())
        val appState =
            QuartermasterAppState(
                sessionStore = FakeSessionStore(accessToken = null, refreshToken = null),
                backend = backend,
            )

        appState.bootstrap()
        appState.handleReminderPayload(reminderPayload())

        assertEquals(AppPhase.Unauthenticated, appState.phase)
        assertEquals(MainTab.Inventory, appState.selectedTab)
        assertEquals("33333333-3333-3333-3333-333333333333", appState.pendingInventoryTarget?.batchId)
        assertTrue(backend.openedReminderIds.isEmpty())
    }

    @Test
    fun `successful addStock clears selection and returns to inventory`() = runTest {
        val backend =
            FakeBackend(
                meResponse = meResponseJson(),
                stock = listOf(stockBatchJson()),
                locations = listOf(locationJson()),
                units = listOf(unitJson("g", "mass")),
                searchResults = listOf(productDtoJson()),
            )
        val appState =
            QuartermasterAppState(
                sessionStore = FakeSessionStore(),
                backend = backend,
            )

        appState.bootstrap()
        appState.searchProducts("flour")
        appState.selectProduct(productDtoJson())
        appState.selectedTab = MainTab.Scan

        appState.addStock(
            productId = "44444444-4444-4444-4444-444444444444",
            locationId = "22222222-2222-2222-2222-222222222222",
            quantity = "500",
            unit = "g",
            expiresOn = "2026-05-01",
            note = "Restocked",
        )

        assertEquals(MainTab.Inventory, appState.selectedTab)
        assertTrue(appState.searchResults.isEmpty())
        assertNull(appState.selectedProduct)
        assertEquals(1, backend.addStockRequests.size)
    }

    @Test
    fun `createLocation validates trims and refreshes location state`() = runTest {
        val backend =
            FakeBackend(
                meResponse = meResponseJson(),
                stock = listOf(stockBatchJson()),
                locations = listOf(locationJson()),
                createdLocation = locationJson(
                    id = "aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa",
                    name = "Shelf",
                    kind = "fridge",
                    sortOrder = 1,
                ),
            )
        val appState = QuartermasterAppState(FakeSessionStore(), backend)

        appState.bootstrap()
        appState.createLocation(LocationFormFields(name = "  Shelf  ", kind = "fridge", sortOrder = 1))

        assertNull(appState.locationActionInFlight)
        assertEquals("Shelf", backend.createLocationRequests.single().name)
        assertEquals("fridge", backend.createLocationRequests.single().kind)
        assertEquals(listOf("Pantry", "Shelf"), appState.sortedLocations().map { it.name })
        assertTrue(appState.hasLoadedInventoryOnce)
    }

    @Test
    fun `createLocation stores validation errors without calling backend`() = runTest {
        val backend = FakeBackend(meResponse = meResponseJson(), locations = listOf(locationJson()))
        val appState = QuartermasterAppState(FakeSessionStore(), backend)

        appState.bootstrap()
        appState.createLocation(LocationFormFields(name = "", kind = "pantry"))

        assertEquals("Enter a location name.", appState.settingsError)
        assertTrue(backend.createLocationRequests.isEmpty())
    }

    @Test
    fun `updateLocation sends full request and refreshes locations`() = runTest {
        val updated =
            locationJson(
                id = "22222222-2222-2222-2222-222222222222",
                name = "Cold Shelf",
                kind = "freezer",
                sortOrder = 7,
            )
        val backend =
            FakeBackend(
                meResponse = meResponseJson(),
                locations = listOf(locationJson()),
                updatedLocation = updated,
            )
        val appState = QuartermasterAppState(FakeSessionStore(), backend)

        appState.bootstrap()
        appState.updateLocation(
            "22222222-2222-2222-2222-222222222222",
            LocationFormFields(name = "Cold Shelf", kind = "freezer", sortOrder = 7),
        )

        val request = backend.updateLocationRequests.single().second
        assertEquals("Cold Shelf", request.name)
        assertEquals("freezer", request.kind)
        assertEquals(7L, request.sortOrder)
        assertEquals(listOf(updated), appState.locations)
    }

    @Test
    fun `deleteLocation removes location after refresh and handles conflict failure`() = runTest {
        val emptyShelf =
            locationJson(
                id = "aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa",
                name = "Empty Shelf",
                sortOrder = 1,
            )
        val backend =
            FakeBackend(
                meResponse = meResponseJson(),
                locations = listOf(locationJson(), emptyShelf),
                deleteLocationFailure = ApiFailure(409, "location_has_stock", "Location has active stock"),
            )
        val appState = QuartermasterAppState(FakeSessionStore(), backend)

        appState.bootstrap()
        appState.deleteLocation(emptyShelf.id.toString())

        assertEquals("Location has active stock", appState.settingsError)
        assertNull(appState.locationActionInFlight)

        backend.deleteLocationFailure = null
        appState.deleteLocation(emptyShelf.id.toString())

        assertEquals(listOf("Pantry"), appState.locations.map { it.name })
        assertEquals(listOf(emptyShelf.id.toString()), backend.deletedLocationIds)
    }

    @Test
    fun `moveLocation swaps target and neighbor sort orders`() = runTest {
        val pantry = locationJson(name = "Pantry", sortOrder = 0)
        val fridge =
            locationJson(
                id = "aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa",
                name = "Fridge",
                kind = "fridge",
                sortOrder = 1,
            )
        val backend = FakeBackend(meResponse = meResponseJson(), locations = listOf(pantry, fridge))
        val appState = QuartermasterAppState(FakeSessionStore(), backend)

        appState.bootstrap()
        appState.moveLocation(fridge.id.toString(), -1)

        val updates = backend.updateLocationRequests
        assertEquals(fridge.id.toString(), updates[0].first)
        assertEquals(0L, updates[0].second.sortOrder)
        assertEquals(pantry.id.toString(), updates[1].first)
        assertEquals(1L, updates[1].second.sortOrder)
        assertEquals(listOf("Fridge", "Pantry"), appState.sortedLocations().map { it.name })
    }

    @Test
    fun `scan product creation handoff returns new product to scan`() = runTest {
        val created = productDtoJson(name = "Semolina")
        val backend =
            FakeBackend(
                meResponse = meResponseJson(),
                locations = listOf(locationJson()),
                units = listOf(unitJson("g", "mass")),
                createdProduct = created,
            )
        val appState = QuartermasterAppState(FakeSessionStore(), backend)

        appState.bootstrap()
        appState.prepareProductCreateForScan()
        val saved = appState.createProduct(ProductFormFields(name = "Semolina", preferredUnit = "g"))

        assertEquals(created, saved)
        assertEquals(MainTab.Scan, appState.selectedTab)
        assertEquals(created, appState.selectedProduct)
        assertFalse(appState.returnToScanAfterProductCreate)
    }

    @Test
    fun `scan product creation cancel returns to scan without selection`() = runTest {
        val appState =
            QuartermasterAppState(
                sessionStore = FakeSessionStore(),
                backend = FakeBackend(meResponse = meResponseJson(), locations = listOf(locationJson())),
            )

        appState.bootstrap()
        appState.prepareProductCreateForScan()
        assertTrue(appState.cancelProductFormForScan())

        assertEquals(MainTab.Scan, appState.selectedTab)
        assertNull(appState.selectedProduct)
        assertFalse(appState.returnToScanAfterProductCreate)
    }

    @Test
    fun `product update request encodes JSON Patch operations`() {
        val patch = ProductUpdateRequest(
            listOf(
                JsonPatchOperation("replace", "/name", "Updated Flour"),
                JsonPatchOperation("remove", "/brand"),
                JsonPatchOperation("replace", "/family", UnitFamily.VOLUME.value),
                JsonPatchOperation("replace", "/preferred_unit", "ml"),
                JsonPatchOperation("remove", "/image_url"),
            ),
        )

        assertEquals(
            """[{"op":"replace","path":"/name","value":"Updated Flour"},{"op":"remove","path":"/brand"},{"op":"replace","path":"/family","value":"volume"},{"op":"replace","path":"/preferred_unit","value":"ml"},{"op":"remove","path":"/image_url"}]""",
            Json(json) { encodeDefaults = false }.encodeToString(patch.operations),
        )
    }

    @Test
    fun `stock update request encodes JSON Patch operations`() {
        val patch = StockUpdateRequest(
            listOf(
                JsonPatchOperation("replace", "/quantity", "750"),
                JsonPatchOperation("replace", "/location_id", "55555555-5555-5555-5555-555555555555"),
                JsonPatchOperation("remove", "/expires_on"),
                JsonPatchOperation("replace", "/opened_on", "2026-04-20"),
                JsonPatchOperation("replace", "/note", "Moved shelf"),
            ),
        )

        assertEquals(
            """[{"op":"replace","path":"/quantity","value":"750"},{"op":"replace","path":"/location_id","value":"55555555-5555-5555-5555-555555555555"},{"op":"remove","path":"/expires_on"},{"op":"replace","path":"/opened_on","value":"2026-04-20"},{"op":"replace","path":"/note","value":"Moved shelf"}]""",
            Json(json) { encodeDefaults = false }.encodeToString(patch.operations),
        )
    }

    @Test
    fun `refreshProducts loads and filters catalogue`() = runTest {
        val active = productDtoJson(name = "Flour")
        val deleted = productDtoJson(
            id = "55555555-5555-5555-5555-555555555555",
            name = "Old beans",
            deletedAt = "2026-04-22T12:00:00Z",
        )
        val appState =
            QuartermasterAppState(
                sessionStore = FakeSessionStore(),
                backend =
                FakeBackend(
                    meResponse = meResponseJson(),
                    products = listOf(active, deleted),
                    units = listOf(unitJson("g", "mass")),
                ),
            )

        appState.bootstrap()

        assertTrue(appState.hasLoadedProductsOnce)
        assertEquals(listOf(active), appState.visibleProducts())

        appState.applyProductFilters("beans", ProductIncludeFilter.Deleted)

        assertEquals("beans", appState.productSearchQuery)
        assertEquals(listOf(deleted), appState.visibleProducts())
    }

    @Test
    fun `manual product create update delete and restore refresh catalogue state`() = runTest {
        val product = productDtoJson(name = "Flour", imageUrl = "https://example.com/flour.png")
        val updated = product.copy(brand = "House", imageUrl = null)
        val deleted = updated.copy(deletedAt = "2026-04-22T12:00:00Z")
        val backend =
            FakeBackend(
                meResponse = meResponseJson(),
                products = listOf(product),
                createdProduct = product,
                updatedProduct = updated,
                restoredProduct = updated,
                deletedProduct = deleted,
                units = listOf(unitJson("g", "mass")),
            )
        val appState =
            QuartermasterAppState(
                sessionStore = FakeSessionStore(),
                backend = backend,
            )

        appState.bootstrap()
        appState.prepareProductCreate()
        val created = appState.createProduct(ProductFormFields(name = "Flour", family = UnitFamily.MASS, preferredUnit = "g"))

        assertEquals(product, created)
        assertEquals(product, appState.selectedCatalogueProduct)
        assertEquals(1, backend.createProductRequests.size)

        val saved = appState.updateSelectedProduct(
            ProductFormFields(
                name = "Flour",
                brand = "House",
                family = UnitFamily.MASS,
                preferredUnit = "g",
                imageUrl = "",
            ),
        )

        assertEquals(updated, saved)
        assertEquals(updated, appState.selectedCatalogueProduct)
        assertTrue(
            backend.updateProductRequests
                .single()
                .operations
                .any { it.op == "remove" && it.path == "/image_url" },
        )

        assertTrue(appState.deleteSelectedProduct())

        assertNull(appState.selectedCatalogueProduct)
        assertEquals(listOf(product.id.toString()), backend.deletedProductIds)

        appState.applyProductFilters("", ProductIncludeFilter.Deleted)
        appState.openProduct(product.id.toString())
        appState.restoreSelectedProduct()

        assertEquals(updated, appState.selectedCatalogueProduct)
        assertEquals(listOf(product.id.toString()), backend.restoredProductIds)
    }

    @Test
    fun `OpenFoodFacts product refresh updates detail`() = runTest {
        val offProduct = productDtoJson(source = "openfoodfacts", barcode = "012345678905")
        val refreshed = offProduct.copy(name = "Fresh OFF Product")
        val appState =
            QuartermasterAppState(
                sessionStore = FakeSessionStore(),
                backend =
                FakeBackend(
                    meResponse = meResponseJson(),
                    products = listOf(offProduct),
                    refreshedProduct = refreshed,
                    units = listOf(unitJson("g", "mass")),
                ),
            )

        appState.bootstrap()
        appState.openProduct(offProduct.id.toString())
        appState.refreshSelectedProductFromOff()

        assertEquals(refreshed, appState.selectedCatalogueProduct)
    }

    @Test
    fun `product action failure clears in flight state and stores product error`() = runTest {
        val product = productDtoJson()
        val appState =
            QuartermasterAppState(
                sessionStore = FakeSessionStore(),
                backend =
                FakeBackend(
                    meResponse = meResponseJson(),
                    products = listOf(product),
                    productUpdateFailure = ApiFailure(409, "product_has_stock", "Product has stock"),
                    units = listOf(unitJson("g", "mass")),
                ),
            )

        appState.bootstrap()
        appState.openProduct(product.id.toString())
        appState.updateSelectedProduct(ProductFormFields(name = "Flour", family = UnitFamily.VOLUME, preferredUnit = "ml"))

        assertNull(appState.productActionInFlight)
        assertEquals("This product still has active stock. Consume or discard it first.", appState.productError)
    }

    @Test
    fun `selectBatch loads batch events and exposes selected batch metadata`() = runTest {
        val batch = stockBatchJson()
        val event = stockEventJson()
        val appState =
            QuartermasterAppState(
                sessionStore = FakeSessionStore(),
                backend =
                FakeBackend(
                    meResponse = meResponseJson(),
                    stock = listOf(batch),
                    locations = listOf(locationJson()),
                    batchEvents = mapOf(batch.id.toString() to listOf(event)),
                ),
            )

        appState.bootstrap()
        appState.selectBatch(batch.id.toString())

        assertEquals(batch.id.toString(), appState.selectedBatchId)
        assertEquals(batch, appState.selectedBatch)
        assertEquals("Pantry", appState.locationNameFor(batch.locationId.toString()))
        assertEquals("Pantry", appState.locationNameFor(batch))
        assertEquals(listOf(event), appState.selectedBatchEvents)
    }

    @Test
    fun `consumeSelectedBatch records request and refreshes inventory reminders and events`() = runTest {
        val batch = stockBatchJson()
        val backend =
            FakeBackend(
                meResponse = meResponseJson(),
                stock = listOf(batch),
                reminders = listOf(reminderJson()),
                locations = listOf(locationJson()),
                batchEvents = mapOf(batch.id.toString() to listOf(stockEventJson(eventType = "consume", quantityDelta = "-100"))),
            )
        val appState =
            QuartermasterAppState(
                sessionStore = FakeSessionStore(),
                backend = backend,
            )

        appState.bootstrap()
        appState.selectBatch(batch.id.toString())
        appState.consumeSelectedBatch("100")

        assertEquals(1, backend.consumeStockRequests.size)
        val request = backend.consumeStockRequests.single()
        assertEquals(batch.product.id, request.productId)
        assertEquals(batch.locationId, request.locationId)
        assertEquals("100", request.quantity)
        assertEquals("g", request.unit)
        assertEquals(batch.id.toString(), appState.selectedBatchId)
        assertTrue(appState.hasLoadedInventoryOnce)
        assertTrue(appState.hasLoadedRemindersOnce)
        assertEquals(StockEventType.CONSUME, appState.selectedBatchEvents.first().eventType)
    }

    @Test
    fun `stock edit fields validate and build JSON Patch operations`() = runTest {
        val pantry = locationJson()
        val fridge = locationJson(id = "55555555-5555-5555-5555-555555555555", name = "Fridge", kind = "fridge", sortOrder = 1)
        val batch = stockBatchJson(expiresOn = "2026-04-30", openedOn = "2026-04-21", note = "Top shelf")
        val appState =
            QuartermasterAppState(
                sessionStore = FakeSessionStore(),
                backend =
                FakeBackend(
                    meResponse = meResponseJson(),
                    stock = listOf(batch),
                    locations = listOf(pantry, fridge),
                ),
            )

        appState.bootstrap()

        assertEquals(
            StockEditFields(
                quantity = "900",
                locationId = pantry.id.toString(),
                expiresOn = "2026-04-30",
                openedOn = "2026-04-21",
                note = "Top shelf",
            ),
            appState.stockEditFields(batch),
        )
        assertEquals("Enter a quantity.", appState.validateStockEditFields(appState.stockEditFields(batch).copy(quantity = "")))
        assertEquals("Enter a positive quantity.", appState.validateStockEditFields(appState.stockEditFields(batch).copy(quantity = "0")))
        assertEquals("Choose a location.", appState.validateStockEditFields(appState.stockEditFields(batch).copy(locationId = "")))
        assertEquals("Enter expiry as YYYY-MM-DD.", appState.validateStockEditFields(appState.stockEditFields(batch).copy(expiresOn = "tomorrow")))
        assertEquals("Enter opened date as YYYY-MM-DD.", appState.validateStockEditFields(appState.stockEditFields(batch).copy(openedOn = "today")))

        val request = appState.stockUpdateRequest(
            batch,
            StockEditFields(
                quantity = "750",
                locationId = fridge.id.toString(),
                expiresOn = "",
                openedOn = "2026-04-22",
                note = "",
            ),
        )

        assertEquals(
            listOf(
                JsonPatchOperation("replace", "/quantity", "750"),
                JsonPatchOperation("replace", "/location_id", fridge.id.toString()),
                JsonPatchOperation("remove", "/expires_on"),
                JsonPatchOperation("replace", "/opened_on", "2026-04-22"),
                JsonPatchOperation("remove", "/note"),
            ),
            request.operations,
        )
    }

    @Test
    fun `updateSelectedBatch patches stock and refreshes inventory reminders and events`() = runTest {
        val batch = stockBatchJson(expiresOn = "2026-04-30", note = "Top shelf")
        val updated = stockBatchJson(quantity = "750", expiresOn = "2026-05-01", note = "Moved shelf")
        val backend =
            FakeBackend(
                meResponse = meResponseJson(),
                stock = listOf(batch),
                reminders = listOf(reminderJson()),
                locations = listOf(locationJson()),
                updatedStock = updated,
                batchEvents = mapOf(batch.id.toString() to listOf(stockEventJson(eventType = "adjust", quantityDelta = "-150"))),
            )
        val appState =
            QuartermasterAppState(
                sessionStore = FakeSessionStore(),
                backend = backend,
            )

        appState.bootstrap()
        appState.selectBatch(batch.id.toString())
        val saved = appState.updateSelectedBatch(
            StockEditFields(
                quantity = "750",
                locationId = batch.locationId.toString(),
                expiresOn = "2026-05-01",
                openedOn = "",
                note = "Moved shelf",
            ),
        )

        assertTrue(saved)
        assertEquals(1, backend.updateStockRequests.size)
        assertEquals(updated, appState.selectedBatch)
        assertEquals(batch.id.toString(), appState.selectedBatchId)
        assertTrue(appState.hasLoadedInventoryOnce)
        assertTrue(appState.hasLoadedRemindersOnce)
        assertEquals(StockEventType.ADJUST, appState.selectedBatchEvents.first().eventType)
        assertNull(appState.stockActionFor(batch.id.toString()))
    }

    @Test
    fun `updateSelectedBatch skips unchanged fields and rejects depleted batches`() = runTest {
        val batch = stockBatchJson()
        val depleted = stockBatchJson(
            id = "55555555-5555-5555-5555-555555555555",
            quantity = "900",
            depletedAt = "2026-04-22T12:30:00Z",
        )
        val backend =
            FakeBackend(
                meResponse = meResponseJson(),
                stock = listOf(batch, depleted),
                locations = listOf(locationJson()),
            )
        val appState =
            QuartermasterAppState(
                sessionStore = FakeSessionStore(),
                backend = backend,
            )

        appState.bootstrap()
        appState.selectBatch(batch.id.toString())

        assertTrue(appState.updateSelectedBatch(appState.stockEditFields(batch)))
        assertEquals(emptyList<StockUpdateRequest>(), backend.updateStockRequests)

        appState.selectBatch(depleted.id.toString())

        assertFalse(appState.canEditBatch(appState.selectedBatch))
        assertFalse(appState.updateSelectedBatch(appState.stockEditFields(depleted)))
        assertEquals(emptyList<StockUpdateRequest>(), backend.updateStockRequests)
    }

    @Test
    fun `stock update failure clears in flight state and stores inventory error`() = runTest {
        val batch = stockBatchJson()
        val appState =
            QuartermasterAppState(
                sessionStore = FakeSessionStore(),
                backend =
                FakeBackend(
                    meResponse = meResponseJson(),
                    stock = listOf(batch),
                    locations = listOf(locationJson()),
                    stockUpdateFailure = ApiFailure(400, "invalid_patch", "Could not update stock"),
                ),
            )

        appState.bootstrap()
        appState.selectBatch(batch.id.toString())
        val saved = appState.updateSelectedBatch(appState.stockEditFields(batch).copy(quantity = "800"))

        assertFalse(saved)
        assertNull(appState.stockActionFor(batch.id.toString()))
        assertEquals("Could not update stock", appState.inventoryError)
        assertEquals("Could not update stock", appState.lastError)
    }

    @Test
    fun `discard and restore update stock action state and restore gating`() = runTest {
        val batch = stockBatchJson()
        val backend =
            FakeBackend(
                meResponse = meResponseJson(),
                stock = listOf(batch),
                locations = listOf(locationJson()),
            )
        val appState =
            QuartermasterAppState(
                sessionStore = FakeSessionStore(),
                backend = backend,
            )

        appState.bootstrap()
        appState.selectBatch(batch.id.toString())
        assertFalse(appState.canRestoreBatch(batch))

        appState.discardBatch(batch.id.toString())

        assertEquals(listOf(batch.id.toString()), backend.discardedBatchIds)
        assertTrue(appState.canRestoreBatch(appState.selectedBatch))

        appState.restoreBatch(batch.id.toString())

        assertEquals(listOf(batch.id.toString()), backend.restoredBatchIds)
        assertFalse(appState.isBatchDepleted(appState.selectedBatch!!))
        assertFalse(appState.canRestoreBatch(appState.selectedBatch))
        assertNull(appState.stockActionFor(batch.id.toString()))
    }

    @Test
    fun `stock action failure clears in flight state and stores inventory error`() = runTest {
        val batch = stockBatchJson()
        val appState =
            QuartermasterAppState(
                sessionStore = FakeSessionStore(),
                backend =
                FakeBackend(
                    meResponse = meResponseJson(),
                    stock = listOf(batch),
                    locations = listOf(locationJson()),
                    discardFailure = ApiFailure(409, "batch_not_restorable", "Could not discard batch"),
                ),
            )

        appState.bootstrap()
        appState.selectBatch(batch.id.toString())
        appState.discardBatch(batch.id.toString())

        assertNull(appState.stockActionFor(batch.id.toString()))
        assertEquals("Could not discard batch", appState.inventoryError)
        assertEquals("Could not discard batch", appState.lastError)
    }

    @Test
    fun `logout clears selected batch history and stock action state`() = runTest {
        val batch = stockBatchJson()
        val appState =
            QuartermasterAppState(
                sessionStore = FakeSessionStore(),
                backend =
                FakeBackend(
                    meResponse = meResponseJson(),
                    stock = listOf(batch),
                    locations = listOf(locationJson()),
                    batchEvents = mapOf(batch.id.toString() to listOf(stockEventJson())),
                ),
            )

        appState.bootstrap()
        appState.selectBatch(batch.id.toString())
        appState.logout()

        assertNull(appState.selectedBatchId)
        assertTrue(appState.selectedBatchEvents.isEmpty())
        assertNull(appState.selectedBatchEventError)
        assertNull(appState.stockActionFor(batch.id.toString()))
    }

    private fun reminderPayload(): ReminderPushPayload = ReminderPushPayload(
        reminderId = "55555555-5555-5555-5555-555555555555",
        batchId = "33333333-3333-3333-3333-333333333333",
        productId = "44444444-4444-4444-4444-444444444444",
        locationId = "22222222-2222-2222-2222-222222222222",
        kind = "expiry",
        title = "Use flour soon",
        body = "Pantry flour expires tomorrow.",
    )

    private fun meResponseJson(
        currentHouseholdJson: String? = householdJson("66666666-6666-6666-6666-666666666666", "Home"),
        householdsJson: List<String> = listOf(householdJson("66666666-6666-6666-6666-666666666666", "Home")),
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
            """.trimIndent(),
        )
    }

    private fun householdJson(
        id: String,
        name: String,
    ): String =
        """
        {
          "id": "$id",
          "name": "$name",
          "timezone": "UTC",
          "role": "admin",
          "joined_at": "2026-04-22T12:00:00Z"
        }
        """.trimIndent()

    private fun householdDetailJson(): HouseholdDetailDto = json.decodeFromString(
        """
            {
              "id": "66666666-6666-6666-6666-666666666666",
              "name": "Home",
              "timezone": "UTC",
              "members": []
            }
        """.trimIndent(),
    )

    private fun locationJson(
        id: String = "22222222-2222-2222-2222-222222222222",
        name: String = "Pantry",
        kind: String = "pantry",
        sortOrder: Long = 0,
    ): LocationDto = json.decodeFromString(
        """
            {
              "id": "$id",
              "name": "$name",
              "kind": "$kind",
              "sort_order": $sortOrder
            }
        """.trimIndent(),
    )

    private fun unitJson(
        code: String,
        family: String,
    ): UnitDto = json.decodeFromString(
        """
            {
              "code": "$code",
              "family": "$family",
              "to_base_milli": 1000
            }
        """.trimIndent(),
    )

    private fun stockBatchJson(
        id: String = "33333333-3333-3333-3333-333333333333",
        quantity: String = "900",
        locationName: String = "Pantry",
        depletedAt: String? = null,
        expiresOn: String? = null,
        openedOn: String? = null,
        note: String? = null,
    ): StockBatchDto = json.decodeFromString(
        """
            {
              "id": "$id",
              "product": {
                "id": "44444444-4444-4444-4444-444444444444",
                "name": "Flour",
                "family": "mass",
                "preferred_unit": "g",
                "source": "manual"
              },
              "location_id": "22222222-2222-2222-2222-222222222222",
              "location_name": "$locationName",
              "initial_quantity": "1000",
              "quantity": "$quantity",
              "unit": "g",
              "created_at": "2026-04-22T12:00:00Z"
              ${depletedAt?.let { ""","depleted_at": "$it"""" } ?: ""}
              ${expiresOn?.let { ""","expires_on": "$it"""" } ?: ""}
              ${openedOn?.let { ""","opened_on": "$it"""" } ?: ""}
              ${note?.let { ""","note": "$it"""" } ?: ""}
            }
        """.trimIndent(),
    )

    private fun stockEventJson(
        id: String = "77777777-7777-7777-7777-777777777777",
        eventType: String = "add",
        quantityDelta: String = "1000",
    ): StockEventDto = json.decodeFromString(
        """
            {
              "id": "$id",
              "event_type": "$eventType",
              "quantity_delta": "$quantityDelta",
              "unit": "g",
              "created_at": "2026-04-22T12:00:00Z",
              "created_by_username": "alice",
              "batch_id": "33333333-3333-3333-3333-333333333333",
              "product": {
                "id": "44444444-4444-4444-4444-444444444444",
                "name": "Flour",
                "family": "mass",
                "preferred_unit": "g",
                "source": "manual"
              }
            }
        """.trimIndent(),
    )

    private fun productDtoJson(
        id: String = "44444444-4444-4444-4444-444444444444",
        name: String = "Flour",
        family: String = "mass",
        preferredUnit: String = "g",
        source: String = "manual",
        brand: String? = null,
        barcode: String? = null,
        imageUrl: String? = null,
        deletedAt: String? = null,
    ): ProductDto {
        val nullableBrand = brand?.let { "\"$it\"" } ?: "null"
        val nullableBarcode = barcode?.let { "\"$it\"" } ?: "null"
        val nullableImageUrl = imageUrl?.let { "\"$it\"" } ?: "null"
        val nullableDeletedAt = deletedAt?.let { "\"$it\"" } ?: "null"
        return json.decodeFromString(
            """
            {
              "id": "$id",
              "name": "$name",
              "family": "$family",
              "preferred_unit": "$preferredUnit",
              "source": "$source",
              "brand": $nullableBrand,
              "barcode": $nullableBarcode,
              "image_url": $nullableImageUrl,
              "deleted_at": $nullableDeletedAt
            }
            """.trimIndent(),
        )
    }

    private fun reminderJson(
        id: String = "55555555-5555-5555-5555-555555555555",
        title: String = "Use flour soon",
    ): ReminderDto = json.decodeFromString(
        """
            {
              "id": "$id",
              "kind": "expiry",
              "title": "$title",
              "body": "Pantry flour expires tomorrow.",
              "fire_at": "2026-04-23T09:00:00Z",
              "household_timezone": "UTC",
              "household_fire_local_at": "2026-04-23T09:00:00",
              "batch_id": "33333333-3333-3333-3333-333333333333",
              "product_id": "44444444-4444-4444-4444-444444444444",
              "location_id": "22222222-2222-2222-2222-222222222222"
            }
        """.trimIndent(),
    )

    private class FakeSessionStore(
        accessToken: String? = "access",
        refreshToken: String? = "refresh",
    ) : SessionStore {
        private var snapshot =
            SessionSnapshot(
                serverUrl = "http://10.0.2.2:8080",
                accessToken = accessToken,
                refreshToken = refreshToken,
            )

        override fun snapshot(): SessionSnapshot = snapshot

        override fun saveServerUrl(url: String) {
            snapshot = snapshot.copy(serverUrl = url)
        }

        override fun saveTokens(
            accessToken: String,
            refreshToken: String,
        ) {
            snapshot = snapshot.copy(accessToken = accessToken, refreshToken = refreshToken)
        }

        override fun clearTokens() {
            snapshot = snapshot.copy(accessToken = null, refreshToken = null)
        }

        override fun stableDeviceId(): String = "android-device-1"
    }

    private inner class FakeBackend(
        private val meResponse: MeResponse,
        stock: List<StockBatchDto> = emptyList(),
        reminders: List<ReminderDto> = emptyList(),
        locations: List<LocationDto> = emptyList(),
        private val units: List<UnitDto> = emptyList(),
        private val householdDetail: HouseholdDetailDto? = null,
        products: List<ProductDto> = emptyList(),
        private val createdLocation: LocationDto? = null,
        private val updatedLocation: LocationDto? = null,
        private val createdProduct: ProductDto? = null,
        private val updatedProduct: ProductDto? = null,
        private val deletedProduct: ProductDto? = null,
        private val restoredProduct: ProductDto? = null,
        private val refreshedProduct: ProductDto? = null,
        private val updatedStock: StockBatchDto? = null,
        private val searchResults: List<ProductDto> = emptyList(),
        batchEvents: Map<String, List<StockEventDto>> = emptyMap(),
        private val barcodeFailure: Throwable? = null,
        private val productUpdateFailure: Throwable? = null,
        private val stockUpdateFailure: Throwable? = null,
        var deleteLocationFailure: Throwable? = null,
        private val openReminderFailure: Throwable? = null,
        private val discardFailure: Throwable? = null,
        private val logoutFailure: Throwable? = null,
    ) : QuartermasterBackend {
        override var serverUrl: String = "http://10.0.2.2:8080"

        private val stockState = stock.toMutableList()
        private val reminderState = reminders.toMutableList()
        private val locationState = locations.toMutableList()
        private val productState = products.toMutableList()
        private val batchEventState = batchEvents.mapValues { it.value.toMutableList() }.toMutableMap()

        val acknowledgedReminderIds = mutableListOf<String>()
        val openedReminderIds = mutableListOf<String>()
        val addStockRequests = mutableListOf<CreateStockRequest>()
        val createLocationRequests = mutableListOf<CreateLocationRequest>()
        val updateLocationRequests = mutableListOf<Pair<String, UpdateLocationRequest>>()
        val deletedLocationIds = mutableListOf<String>()
        val createProductRequests = mutableListOf<CreateProductRequest>()
        val updateProductRequests = mutableListOf<ProductUpdateRequest>()
        val deletedProductIds = mutableListOf<String>()
        val restoredProductIds = mutableListOf<String>()
        val updateStockRequests = mutableListOf<StockUpdateRequest>()
        val consumeStockRequests = mutableListOf<ConsumeRequest>()
        val discardedBatchIds = mutableListOf<String>()
        val restoredBatchIds = mutableListOf<String>()

        override suspend fun me(): MeResponse = meResponse

        override suspend fun login(
            username: String,
            password: String,
        ) = Unit

        override suspend fun register(
            username: String,
            password: String,
            email: String?,
            inviteCode: String?,
        ) = Unit

        override suspend fun logout() {
            logoutFailure?.let { throw it }
        }

        override suspend fun switchHousehold(householdId: String): MeResponse = meResponse

        override suspend fun createHousehold(
            name: String,
            timezone: String,
        ): MeResponse = meResponse

        override suspend fun redeemInvite(inviteCode: String) = Unit

        override suspend fun currentHousehold(): HouseholdDetailDto = householdDetail ?: error("Unused in test")

        override suspend fun householdInvites(): List<InviteDto> = emptyList()

        override suspend fun createInvite(body: CreateInviteRequest): InviteDto {
            error("Unused in test")
        }

        override suspend fun locations(): List<LocationDto> = locationState.toList()

        override suspend fun createLocation(request: CreateLocationRequest): LocationDto {
            createLocationRequests += request
            val location = createdLocation ?: locationJson(
                id = "aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa",
                name = request.name,
                kind = request.kind,
                sortOrder = request.sortOrder ?: ((locationState.maxOfOrNull { it.sortOrder } ?: -1L) + 1L),
            )
            locationState.removeAll { it.id == location.id }
            locationState += location
            return location
        }

        override suspend fun updateLocation(
            id: String,
            request: UpdateLocationRequest,
        ): LocationDto {
            updateLocationRequests += id to request
            val location = updatedLocation?.takeIf { it.id.toString() == id } ?: locationJson(
                id = id,
                name = request.name,
                kind = request.kind,
                sortOrder = request.sortOrder,
            )
            locationState.removeAll { it.id.toString() == id }
            locationState += location
            return location
        }

        override suspend fun deleteLocation(id: String) {
            deleteLocationFailure?.let { throw it }
            deletedLocationIds += id
            locationState.removeAll { it.id.toString() == id }
        }

        override suspend fun units(): List<UnitDto> = units

        override suspend fun listStock(includeDepleted: Boolean): List<StockBatchDto> = if (includeDepleted) {
            stockState.toList()
        } else {
            stockState.filterNot { it.quantity.toBigDecimalOrNull()?.compareTo(java.math.BigDecimal.ZERO) == 0 }
        }

        override suspend fun listEvents(limit: Int): List<StockEventDto> = emptyList()

        override suspend fun listBatchEvents(batchId: String, limit: Int): List<StockEventDto> = batchEventState[batchId].orEmpty().take(limit)

        override suspend fun listReminders(limit: Int): List<ReminderDto> = reminderState.toList()

        override suspend fun acknowledgeReminder(id: String) {
            acknowledgedReminderIds += id
            reminderState.removeAll { it.id.toString() == id }
        }

        override suspend fun presentReminder(id: String) = Unit

        override suspend fun openReminder(id: String) {
            openReminderFailure?.let { throw it }
            openedReminderIds += id
            reminderState.removeAll { it.id.toString() == id }
        }

        override suspend fun registerDevice(
            deviceId: String,
            pushToken: String?,
            authorization: PushAuthorizationStatus,
            appVersion: String,
        ) = Unit

        override suspend fun searchProducts(query: String): List<ProductDto> = searchResults

        override suspend fun listProducts(
            query: String?,
            limit: Int,
            includeDeleted: Boolean,
        ): List<ProductDto> = productState
            .filter { includeDeleted || it.deletedAt == null }
            .filter { query.isNullOrBlank() || it.name.contains(query, ignoreCase = true) }
            .take(limit)

        override suspend fun getProduct(id: String): ProductDto = productState.firstOrNull { it.id.toString() == id } ?: error("Unused in test")

        override suspend fun createProduct(request: CreateProductRequest): ProductDto {
            createProductRequests += request
            val product = createdProduct ?: productDtoJson(name = request.name, family = request.family.value, preferredUnit = request.preferredUnit ?: "g")
            productState.removeAll { it.id == product.id }
            productState += product
            return product
        }

        override suspend fun updateProduct(
            id: String,
            request: ProductUpdateRequest,
        ): ProductDto {
            productUpdateFailure?.let { throw it }
            updateProductRequests += request
            val product = updatedProduct ?: getProduct(id)
            productState.removeAll { it.id.toString() == id }
            productState += product
            return product
        }

        override suspend fun deleteProduct(id: String) {
            deletedProductIds += id
            val product = deletedProduct ?: productState.firstOrNull { it.id.toString() == id }?.copy(deletedAt = "2026-04-22T12:00:00Z")
            productState.removeAll { it.id.toString() == id }
            product?.let { productState += it }
        }

        override suspend fun restoreProduct(id: String): ProductDto {
            restoredProductIds += id
            val product = restoredProduct ?: productState.firstOrNull { it.id.toString() == id }?.copy(deletedAt = null) ?: error("Unused in test")
            productState.removeAll { it.id.toString() == id }
            productState += product
            return product
        }

        override suspend fun refreshProduct(id: String): ProductDto {
            val product = refreshedProduct ?: getProduct(id)
            productState.removeAll { it.id.toString() == id }
            productState += product
            return product
        }

        override suspend fun lookupBarcode(barcode: String): BarcodeLookupResponse {
            barcodeFailure?.let { throw it }
            error("Unused in test")
        }

        override suspend fun addStock(request: CreateStockRequest): StockBatchDto {
            addStockRequests += request
            return stockState.firstOrNull() ?: error("Unused in test")
        }

        override suspend fun updateStock(
            id: String,
            request: StockUpdateRequest,
        ): StockBatchDto {
            stockUpdateFailure?.let { throw it }
            updateStockRequests += request
            val current = stockState.firstOrNull { it.id.toString() == id } ?: error("Unused in test")
            val updated = updatedStock?.takeIf { it.id.toString() == id } ?: current
            stockState.removeAll { it.id.toString() == id }
            stockState += updated
            batchEventState[id] = (
                listOf(
                    stockEventJson(
                        id = "cccccccc-cccc-cccc-cccc-cccccccccccc",
                        eventType = "adjust",
                        quantityDelta = "0",
                    ),
                ) + batchEventState[id].orEmpty()
                ).toMutableList()
            return updated
        }

        override suspend fun consumeStock(request: ConsumeRequest): ConsumeResponse {
            consumeStockRequests += request
            val batch = stockState.firstOrNull { it.product.id == request.productId && it.locationId == request.locationId }
                ?: error("Unused in test")
            batchEventState[batch.id.toString()] = (
                listOf(
                    stockEventJson(
                        id = "88888888-8888-8888-8888-888888888888",
                        eventType = "consume",
                        quantityDelta = "-${request.quantity}",
                    ),
                ) + batchEventState[batch.id.toString()].orEmpty()
                ).toMutableList()
            return ConsumeResponse(
                consumeRequestId = UUID.fromString("99999999-9999-9999-9999-999999999999"),
                consumed = listOf(
                    ConsumedBatchDto(
                        batchId = batch.id,
                        quantity = request.quantity,
                        unit = batch.unit,
                        quantityInRequestedUnit = request.quantity,
                        requestedUnit = request.unit,
                        depleted = false,
                    ),
                ),
            )
        }

        override suspend fun discardStock(batchId: String) {
            discardFailure?.let { throw it }
            discardedBatchIds += batchId
            stockState.replaceAll { batch ->
                if (batch.id.toString() == batchId) {
                    batch.copy(quantity = "0", depletedAt = "2026-04-22T12:30:00Z")
                } else {
                    batch
                }
            }
            batchEventState[batchId] = (
                listOf(
                    stockEventJson(
                        id = "aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa",
                        eventType = "discard",
                        quantityDelta = "-900",
                    ),
                ) + batchEventState[batchId].orEmpty()
                ).toMutableList()
        }

        override suspend fun restoreStock(batchId: String): StockBatchDto {
            restoredBatchIds += batchId
            var restored: StockBatchDto? = null
            stockState.replaceAll { batch ->
                if (batch.id.toString() == batchId) {
                    batch.copy(quantity = batch.initialQuantity, depletedAt = null).also { restored = it }
                } else {
                    batch
                }
            }
            batchEventState[batchId] = (
                listOf(
                    stockEventJson(
                        id = "bbbbbbbb-bbbb-bbbb-bbbb-bbbbbbbbbbbb",
                        eventType = "restore",
                        quantityDelta = "900",
                    ),
                ) + batchEventState[batchId].orEmpty()
                ).toMutableList()
            return restored ?: error("Unused in test")
        }
    }
}
