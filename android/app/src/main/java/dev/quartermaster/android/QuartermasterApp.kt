package dev.quartermaster.android

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.outlined.Category
import androidx.compose.material.icons.outlined.Inventory2
import androidx.compose.material.icons.outlined.Notifications
import androidx.compose.material.icons.outlined.QrCodeScanner
import androidx.compose.material.icons.outlined.Settings
import androidx.compose.material3.Button
import androidx.compose.material3.Card
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.NavigationBar
import androidx.compose.material3.NavigationBarItem
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Scaffold
import androidx.compose.material3.SnackbarHost
import androidx.compose.material3.SnackbarHostState
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.TopAppBar
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.ExperimentalComposeUiApi
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.testTagsAsResourceId
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.unit.dp
import androidx.navigation.NavHostController
import androidx.navigation.NavType
import androidx.navigation.compose.NavHost
import androidx.navigation.compose.composable
import androidx.navigation.compose.currentBackStackEntryAsState
import androidx.navigation.compose.rememberNavController
import androidx.navigation.navArgument
import dev.quartermaster.android.generated.models.LocationDto
import dev.quartermaster.android.generated.models.ProductDto
import dev.quartermaster.android.generated.models.ReminderDto
import dev.quartermaster.android.generated.models.StockBatchDto
import dev.quartermaster.android.generated.models.StockEventDto
import dev.quartermaster.android.generated.models.UnitFamily
import kotlinx.coroutines.launch

private object SmokeTag {
    const val OnboardingScreen = "smoke-onboarding-screen"
    const val InventoryScreen = "smoke-inventory-screen"
    const val ProductsScreen = "smoke-products-screen"
    const val ReminderScreen = "smoke-reminder-screen"
    const val SettingsScreen = "smoke-settings-screen"
    const val ServerUrlField = "smoke-server-url-field"
    const val UsernameField = "smoke-username-field"
    const val PasswordField = "smoke-password-field"
    const val SignInButton = "smoke-sign-in-button"
    const val RemindersTab = "smoke-tab-reminders"
    const val ProductsTab = "smoke-tab-products"
    const val SettingsTab = "smoke-tab-settings"
    const val ProductList = "smoke-product-list"
    const val ProductSearchField = "smoke-product-search-field"
    const val ProductSearchButton = "smoke-product-search-button"
    const val ProductFilterActive = "smoke-product-filter-active"
    const val ProductFilterAll = "smoke-product-filter-all"
    const val ProductFilterDeleted = "smoke-product-filter-deleted"
    const val ProductBarcodeField = "smoke-product-barcode-field"
    const val ProductBarcodeButton = "smoke-product-barcode-button"
    const val ProductCreateButton = "smoke-product-create-button"
    const val ProductNameField = "smoke-product-name-field"
    const val ProductBrandField = "smoke-product-brand-field"
    const val ProductImageUrlField = "smoke-product-image-url-field"
    const val ProductSubmitButton = "smoke-product-submit-button"
    const val ProductEditButton = "smoke-product-edit-button"
    const val ProductDeleteButton = "smoke-product-delete-button"
    const val ProductDeleteConfirmButton = "smoke-product-delete-confirm-button"
    const val ProductRestoreButton = "smoke-product-restore-button"
    const val ProductRefreshButton = "smoke-product-refresh-button"
    const val ReminderOpenedBanner = "smoke-reminder-opened-banner"
    const val ReminderOpenedDismissButton = "smoke-reminder-opened-dismiss"
    const val InviteHandoffCard = "smoke-invite-handoff-card"
    const val SwitchHouseholdHeader = "smoke-switch-household-header"
    const val SignOutButton = "smoke-sign-out-button"
    const val CreateInviteButton = "smoke-create-invite-button"
    const val LocationList = "smoke-location-list"
    const val LocationCreateButton = "smoke-location-create-button"
    const val LocationNameField = "smoke-location-name-field"
    const val LocationKindPantry = "smoke-location-kind-pantry"
    const val LocationKindFridge = "smoke-location-kind-fridge"
    const val LocationKindFreezer = "smoke-location-kind-freezer"
    const val LocationSubmitButton = "smoke-location-submit-button"
    const val StockEditScreen = "smoke-stock-edit-screen"
    const val StockEditQuantity = "smoke-stock-edit-quantity"
    const val StockEditExpires = "smoke-stock-edit-expires"
    const val StockEditOpened = "smoke-stock-edit-opened"
    const val StockEditNote = "smoke-stock-edit-note"
    const val StockEditSave = "smoke-stock-edit-save"
    const val StockEditCancel = "smoke-stock-edit-cancel"

    fun inventoryBatch(id: String) = "smoke-inventory-batch-$id"
    fun selectedBatch(id: String) = "smoke-selected-batch-$id"
    fun batchEditButton(id: String) = "smoke-batch-edit-$id"
    fun batchConsumeField(id: String) = "smoke-batch-consume-quantity-$id"
    fun batchConsumeButton(id: String) = "smoke-batch-consume-$id"
    fun batchDiscardButton(id: String) = "smoke-batch-discard-$id"
    fun batchRestoreButton(id: String) = "smoke-batch-restore-$id"
    fun batchHistoryRow(id: String) = "smoke-batch-history-$id"
    fun reminderCard(id: String) = "smoke-reminder-card-$id"
    fun reminderAckButton(id: String) = "smoke-reminder-ack-$id"
    fun reminderOpenButton(id: String) = "smoke-reminder-open-$id"
    fun inviteCode(code: String) = "smoke-invite-code-$code"
    fun reminderTarget(batchId: String) = "smoke-reminder-target-$batchId"
    fun productRow(id: String) = "smoke-product-row-$id"
    fun locationEdit(id: String) = "smoke-location-edit-$id"
    fun locationDelete(id: String) = "smoke-location-delete-$id"
    fun locationDeleteConfirm(id: String) = "smoke-location-delete-confirm-$id"
    fun locationMoveUp(id: String) = "smoke-location-move-up-$id"
    fun locationMoveDown(id: String) = "smoke-location-move-down-$id"
    fun stockEditLocation(id: String) = "smoke-stock-edit-location-$id"
}

private object AppRoute {
    const val Inventory = "inventory"
    const val Products = "products"
    const val Reminders = "reminders"
    const val Scan = "scan"
    const val Settings = "settings"
    const val StockEdit = "inventory/batch/{batchId}/edit"

    fun stockEdit(batchId: String) = "inventory/batch/$batchId/edit"
}

@OptIn(ExperimentalMaterial3Api::class, ExperimentalComposeUiApi::class)
@Composable
fun QuartermasterApp(appState: QuartermasterAppState) {
    val snackbarHostState = remember { SnackbarHostState() }
    val navController = rememberNavController()
    val backStackEntry by navController.currentBackStackEntryAsState()
    val currentRoute = backStackEntry?.destination?.route

    LaunchedEffect(appState.lastError) {
        appState.lastError?.let { snackbarHostState.showSnackbar(it) }
    }
    LaunchedEffect(currentRoute) {
        appState.selectedTab = when (currentRoute) {
            AppRoute.Products -> MainTab.Products
            AppRoute.Reminders -> MainTab.Reminders
            AppRoute.Scan -> MainTab.Scan
            AppRoute.Settings -> MainTab.Settings
            AppRoute.StockEdit,
            AppRoute.Inventory,
            null,
            -> MainTab.Inventory
            else -> appState.selectedTab
        }
    }
    LaunchedEffect(appState.selectedTab, currentRoute, appState.currentHouseholdId) {
        val route = appState.selectedTab.route()
        if (
            appState.currentHouseholdId != null &&
            currentRoute != null &&
            currentRoute != AppRoute.StockEdit &&
            currentRoute != route
        ) {
            navController.navigate(route) {
                popUpTo(navController.graph.startDestinationId) {
                    saveState = true
                }
                launchSingleTop = true
                restoreState = true
            }
        }
    }

    MaterialTheme {
        Scaffold(
            modifier = Modifier.semantics { testTagsAsResourceId = true },
            snackbarHost = { SnackbarHost(hostState = snackbarHostState) },
            topBar = {
                TopAppBar(title = { Text("Quartermaster") })
            },
            bottomBar = {
                if (appState.phase is AppPhase.Authenticated && appState.currentHouseholdId != null) {
                    NavigationBar {
                        listOf(
                            MainTab.Inventory to Pair("Inventory", Icons.Outlined.Inventory2),
                            MainTab.Products to Pair("Products", Icons.Outlined.Category),
                            MainTab.Reminders to Pair("Reminders", Icons.Outlined.Notifications),
                            MainTab.Scan to Pair("Scan", Icons.Outlined.QrCodeScanner),
                            MainTab.Settings to Pair("Settings", Icons.Outlined.Settings),
                        ).forEach { (tab, labelIcon) ->
                            NavigationBarItem(
                                modifier = Modifier.testTag(
                                    when (tab) {
                                        MainTab.Products -> SmokeTag.ProductsTab
                                        MainTab.Reminders -> SmokeTag.RemindersTab
                                        MainTab.Settings -> SmokeTag.SettingsTab
                                        else -> "main-tab-${tab.name.lowercase()}"
                                    },
                                ),
                                selected = appState.selectedTab == tab,
                                onClick = {
                                    appState.selectedTab = tab
                                    navController.navigate(tab.route()) {
                                        popUpTo(navController.graph.startDestinationId) {
                                            saveState = true
                                        }
                                        launchSingleTop = true
                                        restoreState = true
                                    }
                                },
                                icon = { androidx.compose.material3.Icon(labelIcon.second, contentDescription = labelIcon.first) },
                                label = { Text(labelIcon.first) },
                            )
                        }
                    }
                }
            },
        ) { padding ->
            when (val phase = appState.phase) {
                AppPhase.Launching -> CenteredLoading(modifier = Modifier.padding(padding))
                is AppPhase.LaunchFailed -> MessageScreen(
                    title = "Couldn't resume session",
                    message = phase.message,
                    modifier = Modifier.padding(padding),
                )
                AppPhase.Unauthenticated -> OnboardingScreen(appState, Modifier.padding(padding))
                is AppPhase.Authenticated ->
                    if (phase.me.currentHousehold == null) {
                        NoHouseholdScreen(appState, Modifier.padding(padding))
                    } else {
                        AuthenticatedNavHost(
                            appState = appState,
                            navController = navController,
                            modifier = Modifier.padding(padding),
                        )
                    }
            }
        }
    }
}

@Composable
private fun AuthenticatedNavHost(
    appState: QuartermasterAppState,
    navController: NavHostController,
    modifier: Modifier = Modifier,
) {
    NavHost(
        navController = navController,
        startDestination = AppRoute.Inventory,
        modifier = modifier,
    ) {
        composable(AppRoute.Inventory) {
            InventoryScreen(
                appState = appState,
                onEditBatch = { batchId -> navController.navigate(AppRoute.stockEdit(batchId)) },
            )
        }
        composable(AppRoute.Products) { ProductsScreen(appState) }
        composable(AppRoute.Reminders) { ReminderScreen(appState) }
        composable(AppRoute.Scan) { ScanScreen(appState) }
        composable(AppRoute.Settings) { SettingsScreen(appState) }
        composable(
            route = AppRoute.StockEdit,
            arguments = listOf(navArgument("batchId") { type = NavType.StringType }),
        ) { entry ->
            val batchId = entry.arguments?.getString("batchId").orEmpty()
            StockEditScreen(
                appState = appState,
                batchId = batchId,
                onDone = { navController.popBackStack(AppRoute.Inventory, inclusive = false) },
                onCancel = { navController.popBackStack(AppRoute.Inventory, inclusive = false) },
            )
        }
    }
}

private fun MainTab.route(): String = when (this) {
    MainTab.Inventory -> AppRoute.Inventory
    MainTab.Products -> AppRoute.Products
    MainTab.Reminders -> AppRoute.Reminders
    MainTab.Scan -> AppRoute.Scan
    MainTab.Settings -> AppRoute.Settings
}

@Composable
private fun CenteredLoading(modifier: Modifier = Modifier) {
    Column(
        modifier = modifier
            .fillMaxSize()
            .padding(24.dp),
        verticalArrangement = Arrangement.Center,
    ) {
        CircularProgressIndicator()
        Spacer(Modifier.height(16.dp))
        Text("Loading Quartermaster…")
    }
}

@Composable
private fun MessageScreen(
    title: String,
    message: String,
    modifier: Modifier = Modifier,
) {
    Column(
        modifier = modifier
            .fillMaxSize()
            .padding(24.dp),
        verticalArrangement = Arrangement.Center,
    ) {
        Text(title, style = MaterialTheme.typography.headlineSmall)
        Spacer(Modifier.height(12.dp))
        Text(message, style = MaterialTheme.typography.bodyMedium)
    }
}

@Composable
private fun SectionHeader(
    title: String,
    body: String? = null,
) {
    Column(verticalArrangement = Arrangement.spacedBy(4.dp)) {
        Text(title, style = MaterialTheme.typography.titleMedium)
        body?.let { Text(it, style = MaterialTheme.typography.bodyMedium) }
    }
}

@Composable
private fun StatusCard(
    title: String,
    message: String,
    modifier: Modifier = Modifier,
) {
    Card {
        Column(
            modifier = modifier
                .fillMaxWidth()
                .padding(16.dp),
            verticalArrangement = Arrangement.spacedBy(6.dp),
        ) {
            Text(title, style = MaterialTheme.typography.titleMedium)
            Text(message)
        }
    }
}

@Composable
private fun InviteHandoffCard(
    inviteCode: String?,
    onDismiss: (() -> Unit)? = null,
) {
    Card {
        Column(
            modifier = Modifier
                .testTag(SmokeTag.InviteHandoffCard)
                .fillMaxWidth()
                .padding(16.dp),
            verticalArrangement = Arrangement.spacedBy(8.dp),
        ) {
            Text("Invite handoff ready", style = MaterialTheme.typography.titleMedium)
            Text(
                if (inviteCode.isNullOrBlank()) {
                    "Quartermaster opened an invite link. Finish the redeem flow below."
                } else {
                    "Quartermaster opened an invite link for code $inviteCode. Finish the redeem flow below."
                },
                modifier = inviteCode
                    ?.takeIf(String::isNotBlank)
                    ?.let { Modifier.testTag(SmokeTag.inviteCode(it)) }
                    ?: Modifier,
            )
            onDismiss?.let {
                TextButton(onClick = it) {
                    Text("Dismiss")
                }
            }
        }
    }
}

@Composable
private fun OnboardingScreen(appState: QuartermasterAppState, modifier: Modifier = Modifier) {
    val scope = rememberCoroutineScope()
    var username by remember { mutableStateOf("") }
    var password by remember { mutableStateOf("") }
    var email by remember { mutableStateOf("") }
    var inviteCode by remember { mutableStateOf(appState.pendingInviteContext?.inviteCode.orEmpty()) }
    var serverUrl by remember { mutableStateOf(appState.serverUrl) }
    var signInMode by remember { mutableStateOf(true) }

    LaunchedEffect(appState.pendingInviteContext) {
        appState.pendingInviteContext?.let { context ->
            inviteCode = context.inviteCode.orEmpty()
            serverUrl = context.serverUrl ?: appState.serverUrl
            signInMode = false
        }
    }

    LazyColumn(
        modifier = modifier
            .testTag(SmokeTag.OnboardingScreen)
            .fillMaxSize()
            .padding(16.dp),
        verticalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        item {
            Text("Know what’s in your kitchen.", style = MaterialTheme.typography.headlineSmall)
        }
        if (appState.hasPendingInviteHandoff) {
            item {
                InviteHandoffCard(
                    inviteCode = appState.pendingInviteCode,
                    onDismiss = appState::clearPendingInviteContext,
                )
            }
        }
        item {
            Text(
                "On the Android emulator, the default local server URL uses 10.0.2.2 to reach the host machine running Quartermaster.",
                style = MaterialTheme.typography.bodyMedium,
            )
        }
        item {
            Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                Button(onClick = { signInMode = true }) { Text("Sign in") }
                TextButton(onClick = { signInMode = false }) { Text("Get started") }
            }
        }
        item {
            OutlinedTextField(
                value = serverUrl,
                onValueChange = {
                    serverUrl = it
                    appState.updateServerUrl(it)
                },
                label = { Text("Server URL") },
                modifier = Modifier
                    .fillMaxWidth()
                    .testTag(SmokeTag.ServerUrlField),
            )
        }
        item {
            OutlinedTextField(
                value = username,
                onValueChange = { username = it },
                label = { Text("Username") },
                modifier = Modifier
                    .fillMaxWidth()
                    .testTag(SmokeTag.UsernameField),
            )
        }
        item {
            OutlinedTextField(
                value = password,
                onValueChange = { password = it },
                label = { Text("Password") },
                modifier = Modifier
                    .fillMaxWidth()
                    .testTag(SmokeTag.PasswordField),
            )
        }
        if (!signInMode) {
            item {
                OutlinedTextField(
                    value = email,
                    onValueChange = { email = it },
                    label = { Text("Email (optional)") },
                    modifier = Modifier.fillMaxWidth(),
                )
            }
            item {
                OutlinedTextField(
                    value = inviteCode,
                    onValueChange = { inviteCode = it },
                    label = { Text("Invite code (optional)") },
                    modifier = Modifier.fillMaxWidth(),
                )
            }
        }
        item {
            Button(
                modifier = Modifier.testTag(SmokeTag.SignInButton),
                onClick = {
                    scope.launch {
                        if (signInMode) {
                            appState.signIn(username = username, password = password)
                        } else {
                            appState.register(
                                username = username,
                                password = password,
                                email = email.takeIf(String::isNotBlank),
                                inviteCode = inviteCode.takeIf(String::isNotBlank),
                            )
                        }
                    }
                },
                enabled = !appState.authActionInFlight && username.isNotBlank() && password.length >= 8,
            ) {
                Text(if (signInMode) "Sign in" else "Create account")
            }
        }
    }
}

@Composable
private fun NoHouseholdScreen(appState: QuartermasterAppState, modifier: Modifier = Modifier) {
    val scope = rememberCoroutineScope()
    var householdName by remember { mutableStateOf("") }
    var timezone by remember { mutableStateOf("UTC") }
    var inviteCode by remember { mutableStateOf(appState.pendingInviteContext?.inviteCode.orEmpty()) }

    LaunchedEffect(appState.pendingInviteContext) {
        if (!appState.pendingInviteContext?.inviteCode.isNullOrBlank()) {
            inviteCode = appState.pendingInviteContext?.inviteCode.orEmpty()
        }
    }

    LazyColumn(
        modifier = modifier
            .testTag(SmokeTag.InventoryScreen)
            .fillMaxSize()
            .padding(16.dp),
        verticalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        item {
            Text("No household selected", style = MaterialTheme.typography.headlineSmall)
        }
        item {
            Text("Create a household, redeem an invite, or switch into an existing membership.")
        }
        if (appState.hasPendingInviteHandoff) {
            item {
                InviteHandoffCard(
                    inviteCode = appState.pendingInviteCode,
                    onDismiss = appState::clearPendingInviteContext,
                )
            }
        }
        appState.settingsError?.let { message ->
            item { ErrorCard("Household action failed", message) }
        }
        item {
            SectionHeader(
                title = "Create household",
                body = "Open servers can create a new household here. Invite-only servers can still join through the redeem flow below.",
            )
        }
        item {
            Card {
                Column(
                    modifier = Modifier
                        .fillMaxWidth()
                        .padding(16.dp),
                    verticalArrangement = Arrangement.spacedBy(12.dp),
                ) {
                    OutlinedTextField(
                        value = householdName,
                        onValueChange = { householdName = it },
                        label = { Text("Household name") },
                        modifier = Modifier.fillMaxWidth(),
                    )
                    OutlinedTextField(
                        value = timezone,
                        onValueChange = { timezone = it },
                        label = { Text("Timezone") },
                        modifier = Modifier.fillMaxWidth(),
                    )
                    Button(
                        onClick = { scope.launch { appState.createHousehold(householdName, timezone) } },
                        enabled = appState.settingsLoadState != LoadState.Loading && householdName.isNotBlank(),
                    ) {
                        Text(if (appState.settingsLoadState == LoadState.Loading) "Working..." else "Create household")
                    }
                }
            }
        }
        item {
            SectionHeader(
                title = "Redeem invite",
                body = "Use this when another household admin shared a join code with you.",
            )
        }
        item {
            Card {
                Column(
                    modifier = Modifier
                        .fillMaxWidth()
                        .padding(16.dp),
                    verticalArrangement = Arrangement.spacedBy(12.dp),
                ) {
                    OutlinedTextField(
                        value = inviteCode,
                        onValueChange = { inviteCode = it },
                        label = { Text("Invite code") },
                        modifier = Modifier.fillMaxWidth(),
                    )
                    Button(
                        onClick = { scope.launch { appState.redeemInvite(inviteCode.trim()) } },
                        enabled = appState.settingsLoadState != LoadState.Loading && inviteCode.isNotBlank(),
                    ) {
                        Text(if (appState.settingsLoadState == LoadState.Loading) "Working..." else "Redeem invite")
                    }
                }
            }
        }
        item {
            SectionHeader(
                title = "Available households",
                body = "Switch into an existing membership without leaving the app.",
            )
        }
        items(appState.meOrNull?.households.orEmpty(), key = { it.id }) { household ->
            Card {
                Row(
                    modifier = Modifier
                        .fillMaxWidth()
                        .padding(16.dp),
                    horizontalArrangement = Arrangement.SpaceBetween,
                ) {
                    Column {
                        Text(household.name)
                        Text(household.role.name)
                    }
                    TextButton(
                        onClick = { scope.launch { appState.switchHousehold(household.id.toString()) } },
                        enabled = appState.settingsLoadState != LoadState.Loading,
                    ) {
                        Text("Switch")
                    }
                }
            }
        }
    }
}

@Composable
private fun InventoryScreen(
    appState: QuartermasterAppState,
    modifier: Modifier = Modifier,
    onEditBatch: (String) -> Unit = {},
) {
    val scope = rememberCoroutineScope()
    LaunchedEffect(appState.currentHouseholdId) {
        appState.refreshInventory(force = true)
    }

    LazyColumn(
        modifier = modifier
            .testTag(SmokeTag.InventoryScreen)
            .fillMaxSize()
            .padding(16.dp),
        verticalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        item {
            Text(
                appState.meOrNull?.currentHousehold?.name ?: "Inventory",
                style = MaterialTheme.typography.headlineSmall,
            )
        }
        if (appState.isInventoryRefreshing) {
            item {
                StatusCard(
                    title = "Refreshing inventory",
                    message = "Quartermaster is syncing stock, locations, and recent history for this household.",
                )
            }
        }

        when {
            appState.inventoryLoadState == LoadState.Loading && !appState.hasLoadedInventoryOnce -> {
                item {
                    StatusCard(
                        title = "Loading inventory",
                        message = "Fetching locations, batches, and recent stock history for this household.",
                    )
                }
            }
            appState.inventoryError != null && !appState.hasLoadedInventoryOnce -> {
                item { ErrorCard("Couldn't load inventory", appState.inventoryError!!) }
            }
            appState.locations.isEmpty() -> {
                item {
                    StatusCard(
                        title = "No locations yet",
                        message = "Create a location in Settings before adding stock to this household.",
                    )
                }
            }
            appState.batches.isEmpty() && appState.inventoryLoadState != LoadState.Loading -> {
                item {
                    StatusCard(
                        title = "Inventory is empty",
                        message = "Use Scan to search for a product and add your first batch.",
                    )
                }
            }
        }

        appState.pendingInventoryTarget?.let { target ->
            val product = appState.batches.firstOrNull { it.product.id.toString() == target.productId }?.product
            val location = appState.locations.firstOrNull { it.id.toString() == target.locationId }
            item {
                StatusCard(
                    title = "Opened from reminder",
                    message = when {
                        product != null && location != null -> "Showing ${product.name} in ${location.name}. The matching batch stays highlighted until you dismiss this banner."
                        product != null -> "Showing ${product.name}. Quartermaster is still matching the reminder location."
                        else -> "Quartermaster is still loading the stock mentioned in this reminder."
                    },
                    modifier = Modifier.testTag(SmokeTag.ReminderOpenedBanner),
                )
            }
            item {
                TextButton(
                    modifier = Modifier.testTag(SmokeTag.ReminderOpenedDismissButton),
                    onClick = { appState.clearInventoryTarget() },
                ) {
                    Text("Dismiss")
                }
            }
        }

        appState.inventoryError?.let { message ->
            item { ErrorCard("Inventory action failed", message) }
        }

        appState.selectedBatch?.let { batch ->
            item {
                BatchDetailCard(
                    appState = appState,
                    batch = batch,
                    onEdit = { onEditBatch(batch.id.toString()) },
                    onConsume = { quantity -> scope.launch { appState.consumeSelectedBatch(quantity) } },
                    onDiscard = { scope.launch { appState.discardBatch(batch.id.toString()) } },
                    onRestore = { scope.launch { appState.restoreBatch(batch.id.toString()) } },
                    onClose = appState::clearSelectedBatch,
                )
            }
        }

        val target = appState.pendingInventoryTarget
        val prioritizedLocations = appState.locations.sortedWith(
            compareByDescending<LocationDto> { it.id.toString() == target?.locationId }.thenBy { it.name.lowercase() },
        )

        items(prioritizedLocations, key = { it.id }) { location ->
            LocationInventoryCard(
                location = location,
                batches = appState.batchesForLocation(location.id.toString(), target),
                target = target,
                selectedBatchId = appState.selectedBatchId,
                isBatchDepleted = appState::isBatchDepleted,
                onSelectBatch = { batchId -> scope.launch { appState.selectBatch(batchId) } },
            )
        }

        if (appState.history.isNotEmpty()) {
            item {
                Text("Recent history", style = MaterialTheme.typography.titleMedium)
            }
            items(appState.history.take(10), key = { it.id }) { event ->
                Card {
                    Column(
                        modifier = Modifier
                            .fillMaxWidth()
                            .padding(16.dp),
                        verticalArrangement = Arrangement.spacedBy(4.dp),
                    ) {
                        Text("${event.product.name} · ${event.eventType.name}")
                        Text("${event.quantityDelta} ${event.unit}")
                    }
                }
            }
        }
    }
}

@Composable
private fun LocationInventoryCard(
    location: LocationDto,
    batches: List<StockBatchDto>,
    target: InventoryTarget?,
    selectedBatchId: String?,
    isBatchDepleted: (StockBatchDto) -> Boolean,
    onSelectBatch: (String) -> Unit,
) {
    val isTargetLocation = location.id.toString() == target?.locationId
    Card {
        Column(
            modifier = Modifier
                .fillMaxWidth()
                .padding(16.dp),
            verticalArrangement = Arrangement.spacedBy(8.dp),
        ) {
            Text(
                if (isTargetLocation) "${location.name} · reminder target" else location.name,
                style = MaterialTheme.typography.titleMedium,
            )
            if (batches.isEmpty()) {
                Text("Nothing here yet.")
            } else {
                batches.forEach { batch ->
                    val batchId = batch.id.toString()
                    val depleted = isBatchDepleted(batch)
                    val isTargetBatch =
                        batchId == target?.batchId ||
                            (batch.product.id.toString() == target?.productId && batch.locationId.toString() == target.locationId)
                    val isSelected = batchId == selectedBatchId
                    Card(
                        colors = CardDefaults.cardColors(
                            containerColor = when {
                                isTargetBatch || isSelected -> MaterialTheme.colorScheme.secondaryContainer
                                depleted -> MaterialTheme.colorScheme.surface
                                else -> MaterialTheme.colorScheme.surfaceVariant
                            },
                        ),
                    ) {
                        Column(
                            modifier = Modifier
                                .testTag(SmokeTag.inventoryBatch(batchId))
                                .fillMaxWidth()
                                .clickable { onSelectBatch(batchId) }
                                .padding(12.dp),
                            verticalArrangement = Arrangement.spacedBy(2.dp),
                        ) {
                            Text("${batch.product.name} · ${batch.quantity} ${batch.unit}")
                            if (depleted) {
                                Text("Depleted", style = MaterialTheme.typography.labelMedium)
                            }
                            batch.expiresOn?.let { Text("Expires $it") }
                            batch.note?.takeIf(String::isNotBlank)?.let { Text(it) }
                            if (isTargetBatch) {
                                Text(
                                    "Reminder target",
                                    style = MaterialTheme.typography.labelMedium,
                                    modifier = Modifier.testTag(SmokeTag.reminderTarget(batchId)),
                                )
                            }
                            if (isSelected) {
                                Text("Selected", style = MaterialTheme.typography.labelMedium)
                            }
                        }
                    }
                }
            }
        }
    }
}

@Composable
private fun BatchDetailCard(
    appState: QuartermasterAppState,
    batch: StockBatchDto,
    onEdit: () -> Unit,
    onConsume: (String) -> Unit,
    onDiscard: () -> Unit,
    onRestore: () -> Unit,
    onClose: () -> Unit,
) {
    var consumeQuantity by remember(batch.id) { mutableStateOf("") }
    val batchId = batch.id.toString()
    val depleted = appState.isBatchDepleted(batch)
    val action = appState.stockActionFor(batchId)
    val consumeQuantityNumber = consumeQuantity.trim().toDoubleOrNull()
    val consumeDisabledReason = when {
        depleted -> "This batch is depleted."
        consumeQuantity.isBlank() -> "Enter an amount to consume."
        consumeQuantityNumber == null || consumeQuantityNumber <= 0 -> "Enter a positive amount."
        else -> null
    }

    Card {
        Column(
            modifier = Modifier
                .testTag(SmokeTag.selectedBatch(batchId))
                .fillMaxWidth()
                .padding(16.dp),
            verticalArrangement = Arrangement.spacedBy(10.dp),
        ) {
            Row(
                modifier = Modifier.fillMaxWidth(),
                horizontalArrangement = Arrangement.SpaceBetween,
            ) {
                Column(modifier = Modifier.weight(1f)) {
                    Text(batch.product.name, style = MaterialTheme.typography.titleLarge)
                    Text("${batch.quantity} ${batch.unit} in ${appState.locationNameFor(batch.locationId.toString())}")
                }
                TextButton(onClick = onClose) {
                    Text("Close")
                }
            }
            BatchMetadata(batch)
            if (depleted) {
                Text("This batch is depleted.", style = MaterialTheme.typography.bodyMedium)
            } else {
                Button(
                    onClick = onEdit,
                    enabled = action == null,
                    modifier = Modifier.testTag(SmokeTag.batchEditButton(batchId)),
                ) {
                    Text(if (action == StockAction.Update) "Saving..." else "Edit")
                }
                OutlinedTextField(
                    value = consumeQuantity,
                    onValueChange = { consumeQuantity = it },
                    label = { Text("Consume quantity (${batch.unit})") },
                    keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Decimal),
                    modifier = Modifier
                        .testTag(SmokeTag.batchConsumeField(batchId))
                        .fillMaxWidth(),
                )
                consumeDisabledReason?.let { Text(it, style = MaterialTheme.typography.bodySmall) }
                Button(
                    onClick = { onConsume(consumeQuantity.trim()) },
                    enabled = consumeDisabledReason == null && action == null,
                    modifier = Modifier.testTag(SmokeTag.batchConsumeButton(batchId)),
                ) {
                    Text(if (action == StockAction.Consume) "Consuming..." else "Consume")
                }
                TextButton(
                    onClick = onDiscard,
                    enabled = action == null,
                    modifier = Modifier.testTag(SmokeTag.batchDiscardButton(batchId)),
                ) {
                    Text(if (action == StockAction.Discard) "Discarding..." else "Discard batch")
                }
            }
            if (appState.canRestoreBatch(batch)) {
                Button(
                    onClick = onRestore,
                    enabled = action == null,
                    modifier = Modifier.testTag(SmokeTag.batchRestoreButton(batchId)),
                ) {
                    Text(if (action == StockAction.Restore) "Restoring..." else "Restore batch")
                }
            }
            BatchHistory(appState.selectedBatchEvents, appState.selectedBatchEventLoadState, appState.selectedBatchEventError)
        }
    }
}

@Composable
private fun BatchMetadata(batch: StockBatchDto) {
    Column(verticalArrangement = Arrangement.spacedBy(3.dp)) {
        Text("Created ${batch.createdAt}", style = MaterialTheme.typography.bodySmall)
        Text("Initial quantity ${batch.initialQuantity} ${batch.unit}", style = MaterialTheme.typography.bodySmall)
        Text("Expires ${batch.expiresOn ?: "No expiry date"}", style = MaterialTheme.typography.bodySmall)
        Text("Opened ${batch.openedOn ?: "Not marked opened"}", style = MaterialTheme.typography.bodySmall)
        batch.note?.takeIf(String::isNotBlank)?.let {
            Text("Note: $it", style = MaterialTheme.typography.bodySmall)
        }
    }
}

@Composable
private fun BatchHistory(
    events: List<StockEventDto>,
    loadState: LoadState,
    error: String?,
) {
    Column(verticalArrangement = Arrangement.spacedBy(6.dp)) {
        Text("Batch history", style = MaterialTheme.typography.titleMedium)
        when {
            loadState == LoadState.Loading && events.isEmpty() -> Text("Loading history...")
            error != null -> Text(error, style = MaterialTheme.typography.bodyMedium)
            events.isEmpty() -> Text("No history yet.")
            else -> events.take(8).forEach { event ->
                Column(
                    modifier = Modifier
                        .testTag(SmokeTag.batchHistoryRow(event.id.toString()))
                        .fillMaxWidth(),
                    verticalArrangement = Arrangement.spacedBy(2.dp),
                ) {
                    Text("${event.eventType.name.lowercase()} · ${event.quantityDelta} ${event.unit}")
                    Text(
                        listOfNotNull(event.createdAt, event.createdByUsername).joinToString(" · "),
                        style = MaterialTheme.typography.bodySmall,
                    )
                    event.note?.takeIf(String::isNotBlank)?.let { Text(it, style = MaterialTheme.typography.bodySmall) }
                }
            }
        }
    }
}

@Composable
private fun StockEditScreen(
    appState: QuartermasterAppState,
    batchId: String,
    onDone: () -> Unit,
    onCancel: () -> Unit,
) {
    val scope = rememberCoroutineScope()
    val batch = appState.batches.firstOrNull { it.id.toString() == batchId }
    var fields by remember(batch?.id) {
        mutableStateOf(batch?.let(appState::stockEditFields) ?: StockEditFields())
    }
    val action = appState.stockActionFor(batchId)
    val validation = batch?.let { appState.validateStockEditFields(fields) }
    val canSave = batch != null && appState.canEditBatch(batch) && validation == null && action == null

    LaunchedEffect(batchId) {
        if (appState.selectedBatchId != batchId) {
            appState.selectBatch(batchId)
        }
    }

    LazyColumn(
        modifier = Modifier
            .testTag(SmokeTag.StockEditScreen)
            .fillMaxSize()
            .padding(16.dp),
        verticalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        item {
            TextButton(
                modifier = Modifier.testTag(SmokeTag.StockEditCancel),
                onClick = onCancel,
            ) {
                Text("Cancel")
            }
        }
        if (batch == null) {
            item { StatusCard("Batch unavailable", "Return to Inventory and choose another batch.") }
            return@LazyColumn
        }
        if (!appState.canEditBatch(batch)) {
            item { StatusCard("Batch is depleted", "Depleted batches cannot be corrected from Android yet.") }
            return@LazyColumn
        }
        item {
            Text("Edit ${batch.product.name}", style = MaterialTheme.typography.headlineSmall)
        }
        appState.inventoryError?.let { message ->
            item { ErrorCard("Stock correction failed", message) }
        }
        item {
            OutlinedTextField(
                value = fields.quantity,
                onValueChange = { fields = fields.copy(quantity = it) },
                label = { Text("Quantity (${batch.unit})") },
                keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Decimal),
                modifier = Modifier
                    .fillMaxWidth()
                    .testTag(SmokeTag.StockEditQuantity),
            )
        }
        item {
            Card {
                Column(
                    modifier = Modifier
                        .fillMaxWidth()
                        .padding(16.dp),
                    verticalArrangement = Arrangement.spacedBy(8.dp),
                ) {
                    Text("Location", style = MaterialTheme.typography.titleMedium)
                    appState.sortedLocations().forEach { location ->
                        val locationId = location.id.toString()
                        Row(
                            modifier = Modifier.fillMaxWidth(),
                            horizontalArrangement = Arrangement.SpaceBetween,
                        ) {
                            Text(location.name)
                            if (fields.locationId == locationId) {
                                Text("Selected", style = MaterialTheme.typography.labelMedium)
                            } else {
                                TextButton(
                                    modifier = Modifier.testTag(SmokeTag.stockEditLocation(locationId)),
                                    onClick = { fields = fields.copy(locationId = locationId) },
                                ) {
                                    Text("Select")
                                }
                            }
                        }
                    }
                }
            }
        }
        item {
            OutlinedTextField(
                value = fields.expiresOn,
                onValueChange = { fields = fields.copy(expiresOn = it) },
                label = { Text("Expires on (YYYY-MM-DD)") },
                modifier = Modifier
                    .fillMaxWidth()
                    .testTag(SmokeTag.StockEditExpires),
            )
        }
        item {
            OutlinedTextField(
                value = fields.openedOn,
                onValueChange = { fields = fields.copy(openedOn = it) },
                label = { Text("Opened on (YYYY-MM-DD)") },
                modifier = Modifier
                    .fillMaxWidth()
                    .testTag(SmokeTag.StockEditOpened),
            )
        }
        item {
            OutlinedTextField(
                value = fields.note,
                onValueChange = { fields = fields.copy(note = it) },
                label = { Text("Note") },
                modifier = Modifier
                    .fillMaxWidth()
                    .testTag(SmokeTag.StockEditNote),
            )
        }
        validation?.let { item { Text(it, style = MaterialTheme.typography.bodySmall) } }
        item {
            Button(
                modifier = Modifier.testTag(SmokeTag.StockEditSave),
                onClick = {
                    scope.launch {
                        if (appState.updateSelectedBatch(fields)) {
                            onDone()
                        }
                    }
                },
                enabled = canSave,
            ) {
                Text(if (action == StockAction.Update) "Saving..." else "Save")
            }
        }
    }
}

@Composable
private fun ReminderScreen(appState: QuartermasterAppState, modifier: Modifier = Modifier) {
    val scope = rememberCoroutineScope()

    LaunchedEffect(appState.currentHouseholdId) {
        appState.refreshReminders(limit = 50)
    }

    LazyColumn(
        modifier = modifier
            .testTag(SmokeTag.ReminderScreen)
            .fillMaxSize()
            .padding(16.dp),
        verticalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        item { Text("Reminders", style = MaterialTheme.typography.headlineSmall) }

        when {
            appState.remindersLoadState == LoadState.Loading && appState.reminders.isEmpty() -> {
                item {
                    StatusCard(
                        title = "Loading reminders",
                        message = "Fetching the household reminder inbox and marking unseen items as presented on this device.",
                    )
                }
            }
            appState.reminderError != null && appState.reminders.isEmpty() -> {
                item { ErrorCard("Couldn't load reminders", appState.reminderError!!) }
            }
            appState.reminders.isEmpty() -> {
                item {
                    StatusCard(
                        title = "No due reminders",
                        message = "Expiry reminders stay here until someone opens or acknowledges them.",
                    )
                }
            }
        }
        if (appState.isRemindersRefreshing && appState.reminders.isNotEmpty()) {
            item {
                StatusCard(
                    title = "Refreshing reminders",
                    message = "Quartermaster is syncing the latest due reminders for this household.",
                )
            }
        }

        items(appState.reminders, key = { it.id }) { reminder ->
            ReminderCard(
                reminder = reminder,
                action = appState.reminderActionFor(reminder.id.toString()),
                onOpen = { scope.launch { appState.openReminder(reminder) } },
                onAcknowledge = { scope.launch { appState.acknowledgeReminder(reminder.id.toString()) } },
            )
        }
    }
}

@Composable
private fun ReminderCard(
    reminder: ReminderDto,
    action: ReminderAction?,
    onOpen: () -> Unit,
    onAcknowledge: () -> Unit,
) {
    Card {
        Column(
            modifier = Modifier
                .testTag(SmokeTag.reminderCard(reminder.id.toString()))
                .fillMaxWidth()
                .padding(16.dp),
            verticalArrangement = Arrangement.spacedBy(8.dp),
        ) {
            Text(reminder.title, style = MaterialTheme.typography.titleMedium)
            Text(reminder.body)
            if (action != null) {
                Text(
                    if (action == ReminderAction.Open) {
                        "Opening reminder and refreshing inventory…"
                    } else {
                        "Acknowledging reminder and removing it from the inbox…"
                    },
                    style = MaterialTheme.typography.bodySmall,
                )
            }
            Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                Button(
                    modifier = Modifier.testTag(SmokeTag.reminderOpenButton(reminder.id.toString())),
                    onClick = onOpen,
                    enabled = action == null,
                ) {
                    Text(if (action == ReminderAction.Open) "Opening..." else "Open")
                }
                TextButton(
                    modifier = Modifier.testTag(SmokeTag.reminderAckButton(reminder.id.toString())),
                    onClick = onAcknowledge,
                    enabled = action == null,
                ) {
                    Text(if (action == ReminderAction.Acknowledge) "Acknowledging..." else "Acknowledge")
                }
            }
        }
    }
}

@Composable
private fun ScanScreen(appState: QuartermasterAppState, modifier: Modifier = Modifier) {
    val scope = rememberCoroutineScope()
    var barcode by remember { mutableStateOf("") }
    var query by remember { mutableStateOf("") }
    var quantity by remember { mutableStateOf("") }
    var unit by remember { mutableStateOf("") }
    var selectedLocationId by remember { mutableStateOf<String?>(null) }
    var expiresOn by remember { mutableStateOf("") }
    var note by remember { mutableStateOf("") }

    LaunchedEffect(appState.currentHouseholdId) {
        appState.refreshInventory(force = appState.locations.isEmpty())
    }

    val selectedProduct = appState.selectedProduct
    val locations = appState.locations.sortedWith(compareBy<LocationDto> { it.sortOrder }.thenBy { it.name.lowercase() })
    val unitChoices = selectedProduct?.let(appState::unitSymbolsFor).orEmpty()
    val selectedUnit = unit.ifBlank { selectedProduct?.let(appState::defaultUnitSymbolFor).orEmpty() }
    val selectedLocation = selectedLocationId?.let { id -> locations.firstOrNull { it.id.toString() == id } }
    val addDisabledReason = when {
        selectedProduct == null -> "Choose a product before you try to add stock."
        locations.isEmpty() -> "Create a household location in Settings before adding stock."
        selectedLocation == null -> "Choose where this batch lives before saving it."
        quantity.isBlank() -> "Enter how much stock you are adding."
        selectedUnit.isBlank() -> "Choose the unit that matches this product family."
        else -> null
    }

    LaunchedEffect(locations.map { it.id }) {
        if (selectedLocationId == null || locations.none { it.id.toString() == selectedLocationId }) {
            selectedLocationId = locations.firstOrNull()?.id?.toString()
        }
    }

    LaunchedEffect(selectedProduct?.id, unitChoices) {
        if (selectedProduct == null) {
            unit = ""
        } else if (unit.isBlank() || unit !in unitChoices) {
            unit = appState.defaultUnitSymbolFor(selectedProduct).orEmpty()
        }
    }

    LazyColumn(
        modifier = modifier
            .fillMaxSize()
            .padding(16.dp),
        verticalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        item { Text("Scan & add stock", style = MaterialTheme.typography.headlineSmall) }
        item {
            Text(
                "The emulator default server uses 10.0.2.2 to reach Quartermaster on this machine. Override the server URL in onboarding for a phone or remote server.",
                style = MaterialTheme.typography.bodySmall,
            )
        }
        appState.inventoryError?.let { message ->
            item { ErrorCard("Inventory refresh failed", message) }
        }
        appState.scanError?.let { message ->
            item { ErrorCard("Scan action failed", message) }
        }
        item {
            SectionHeader(
                title = "1. Find a product",
                body = "Look up a barcode or search the product catalog before you add stock.",
            )
        }
        item {
            Button(
                onClick = appState::showProductCreateForScan,
                enabled = appState.scanActionInFlight == null,
            ) {
                Text("Create manual product")
            }
        }
        item {
            Card {
                Column(
                    modifier = Modifier
                        .fillMaxWidth()
                        .padding(16.dp),
                    verticalArrangement = Arrangement.spacedBy(12.dp),
                ) {
                    OutlinedTextField(
                        value = barcode,
                        onValueChange = { barcode = it },
                        label = { Text("Barcode") },
                        keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Number),
                        modifier = Modifier.fillMaxWidth(),
                    )
                    Button(
                        onClick = { scope.launch { appState.lookupBarcode(barcode.trim()) } },
                        enabled = barcode.isNotBlank() && appState.scanActionInFlight == null,
                    ) {
                        Text(if (appState.scanActionInFlight == ScanAction.BarcodeLookup) "Looking up..." else "Look up barcode")
                    }
                    OutlinedTextField(
                        value = query,
                        onValueChange = { query = it },
                        label = { Text("Search products") },
                        modifier = Modifier.fillMaxWidth(),
                    )
                    Button(
                        onClick = { scope.launch { appState.searchProducts(query.trim()) } },
                        enabled = query.isNotBlank() && appState.scanActionInFlight == null,
                    ) {
                        Text(if (appState.scanActionInFlight == ScanAction.ProductSearch) "Searching..." else "Search")
                    }
                }
            }
        }
        items(appState.searchResults, key = { it.id }) { product ->
            ProductSearchResultCard(product) { appState.selectProduct(product) }
        }
        if (selectedProduct != null) {
            item {
                SectionHeader(
                    title = "2. Add ${selectedProduct.name}",
                    body = "Choose where this stock lives, confirm the unit, then save the batch.",
                )
            }
            item {
                Card {
                    Column(
                        modifier = Modifier
                            .fillMaxWidth()
                            .padding(16.dp),
                        verticalArrangement = Arrangement.spacedBy(12.dp),
                    ) {
                        SelectionCard(
                            title = "Location",
                            options = locations.map { it.id.toString() to it.name },
                            selected = selectedLocationId,
                            emptyText = "No locations yet. Add a location from Settings first.",
                            onSelect = { selectedLocationId = it },
                        )
                        OutlinedTextField(
                            value = quantity,
                            onValueChange = { quantity = it },
                            label = { Text("Quantity") },
                            modifier = Modifier.fillMaxWidth(),
                        )
                        SelectionCard(
                            title = "Unit",
                            options = unitChoices.map { it to it },
                            selected = selectedUnit.takeIf(String::isNotBlank),
                            emptyText = "No units are available for ${selectedProduct.family.name.lowercase()} products.",
                            onSelect = { unit = it },
                        )
                        Text(
                            if (selectedUnit.isBlank()) {
                                "No unit selected yet."
                            } else {
                                "Selected unit: $selectedUnit"
                            },
                            style = MaterialTheme.typography.bodySmall,
                        )
                        OutlinedTextField(
                            value = expiresOn,
                            onValueChange = { expiresOn = it },
                            label = { Text("Expires on (YYYY-MM-DD)") },
                            modifier = Modifier.fillMaxWidth(),
                        )
                        OutlinedTextField(
                            value = note,
                            onValueChange = { note = it },
                            label = { Text("Note") },
                            modifier = Modifier.fillMaxWidth(),
                        )
                        addDisabledReason?.let { Text(it, style = MaterialTheme.typography.bodySmall) }
                        Button(
                            onClick = {
                                selectedLocation?.let { location ->
                                    scope.launch {
                                        appState.addStock(
                                            productId = selectedProduct.id.toString(),
                                            locationId = location.id.toString(),
                                            quantity = quantity.trim(),
                                            unit = selectedUnit.trim(),
                                            expiresOn = expiresOn.takeIf(String::isNotBlank),
                                            note = note.takeIf(String::isNotBlank),
                                        )
                                        quantity = ""
                                        unit = ""
                                        expiresOn = ""
                                        note = ""
                                        selectedLocationId = null
                                    }
                                }
                            },
                            enabled = addDisabledReason == null && appState.scanActionInFlight == null,
                        ) {
                            Text(if (appState.scanActionInFlight == ScanAction.AddStock) "Adding..." else "Add stock")
                        }
                    }
                }
            }
        }
    }
}

@Composable
private fun ProductSearchResultCard(
    product: ProductDto,
    onUse: () -> Unit,
) {
    Card {
        Row(
            modifier = Modifier
                .fillMaxWidth()
                .padding(16.dp),
            horizontalArrangement = Arrangement.SpaceBetween,
        ) {
            Column {
                Text(product.name)
                Text(product.family.name)
            }
            TextButton(onClick = onUse) { Text("Use") }
        }
    }
}

@Composable
private fun ProductsScreen(appState: QuartermasterAppState, modifier: Modifier = Modifier) {
    LaunchedEffect(appState.currentHouseholdId) {
        if (!appState.hasLoadedProductsOnce) {
            appState.refreshProducts(force = true)
        }
    }

    when (appState.productScreenMode) {
        ProductScreenMode.List -> ProductListScreen(appState, modifier)
        ProductScreenMode.Detail -> ProductDetailScreen(appState, modifier)
        ProductScreenMode.Create -> ProductFormScreen(appState, null, modifier)
        ProductScreenMode.Edit -> ProductFormScreen(appState, appState.selectedCatalogueProduct, modifier)
        ProductScreenMode.Delete -> ProductDeleteScreen(appState, modifier)
    }
}

@Composable
private fun ProductListScreen(appState: QuartermasterAppState, modifier: Modifier = Modifier) {
    val scope = rememberCoroutineScope()
    var query by remember { mutableStateOf(appState.productSearchQuery) }
    var barcode by remember { mutableStateOf("") }
    var filter by remember { mutableStateOf(appState.productIncludeFilter) }

    LazyColumn(
        modifier = modifier
            .fillMaxSize()
            .padding(16.dp)
            .testTag(SmokeTag.ProductsScreen),
        verticalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        item {
            Row(
                modifier = Modifier.fillMaxWidth(),
                horizontalArrangement = Arrangement.SpaceBetween,
            ) {
                Text("Products", style = MaterialTheme.typography.headlineSmall)
                Button(
                    modifier = Modifier.testTag(SmokeTag.ProductCreateButton),
                    onClick = appState::showProductCreate,
                    enabled = appState.productActionInFlight == null,
                ) {
                    Text("New product")
                }
            }
        }
        if (appState.isProductsRefreshing) {
            item { StatusCard("Refreshing products", "Quartermaster is syncing the household catalogue.") }
        }
        appState.productError?.let { message ->
            item { ErrorCard("Product action failed", message) }
        }
        item {
            Card {
                Column(
                    modifier = Modifier
                        .fillMaxWidth()
                        .padding(16.dp),
                    verticalArrangement = Arrangement.spacedBy(12.dp),
                ) {
                    OutlinedTextField(
                        value = query,
                        onValueChange = { query = it },
                        label = { Text("Search products") },
                        modifier = Modifier
                            .fillMaxWidth()
                            .testTag(SmokeTag.ProductSearchField),
                    )
                    Text("Include", style = MaterialTheme.typography.titleMedium)
                    Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                        ProductFilterButton("Active", SmokeTag.ProductFilterActive, filter == ProductIncludeFilter.Active) {
                            filter = ProductIncludeFilter.Active
                        }
                        ProductFilterButton("All", SmokeTag.ProductFilterAll, filter == ProductIncludeFilter.All) {
                            filter = ProductIncludeFilter.All
                        }
                        ProductFilterButton("Deleted", SmokeTag.ProductFilterDeleted, filter == ProductIncludeFilter.Deleted) {
                            filter = ProductIncludeFilter.Deleted
                        }
                    }
                    Button(
                        modifier = Modifier.testTag(SmokeTag.ProductSearchButton),
                        onClick = { scope.launch { appState.applyProductFilters(query.trim(), filter) } },
                        enabled = appState.productActionInFlight == null,
                    ) {
                        Text(if (appState.productActionInFlight == ProductAction.LoadList) "Loading..." else "Apply")
                    }
                    OutlinedTextField(
                        value = barcode,
                        onValueChange = { barcode = it },
                        label = { Text("Barcode lookup") },
                        keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Number),
                        modifier = Modifier
                            .fillMaxWidth()
                            .testTag(SmokeTag.ProductBarcodeField),
                    )
                    Button(
                        modifier = Modifier.testTag(SmokeTag.ProductBarcodeButton),
                        onClick = { scope.launch { appState.lookupProductBarcode(barcode.trim()) } },
                        enabled = barcode.isNotBlank() && appState.productActionInFlight == null,
                    ) {
                        Text(if (appState.productActionInFlight == ProductAction.BarcodeLookup) "Looking up..." else "Look up barcode")
                    }
                }
            }
        }
        val products = appState.visibleProducts()
        if (!appState.hasLoadedProductsOnce && appState.productLoadState == LoadState.Loading) {
            item { CenteredLoading() }
        } else if (products.isEmpty()) {
            item { StatusCard("No products found", "Create a manual product or look up a barcode to add one to the catalogue.") }
        } else {
            item {
                Text(
                    "Catalogue",
                    style = MaterialTheme.typography.titleMedium,
                    modifier = Modifier.testTag(SmokeTag.ProductList),
                )
            }
            items(products, key = { it.id }) { product ->
                ProductCatalogueRow(appState, product) {
                    scope.launch { appState.openProduct(product.id.toString()) }
                }
            }
        }
    }
}

@Composable
private fun ProductCatalogueRow(
    appState: QuartermasterAppState,
    product: ProductDto,
    onOpen: () -> Unit,
) {
    Card(
        modifier = Modifier
            .fillMaxWidth()
            .testTag(SmokeTag.productRow(product.id.toString()))
            .clickable(onClick = onOpen),
    ) {
        Column(
            modifier = Modifier.padding(16.dp),
            verticalArrangement = Arrangement.spacedBy(4.dp),
        ) {
            Text(product.name, style = MaterialTheme.typography.titleMedium)
            Text("${product.brand ?: "No brand"} · ${product.family.value} · ${product.preferredUnit}")
            Text("${appState.productSourceLabel(product)} · ${if (appState.isDeletedProduct(product)) "Deleted" else "Active"}")
        }
    }
}

@Composable
private fun ProductFilterButton(
    label: String,
    tag: String,
    selected: Boolean,
    onClick: () -> Unit,
) {
    if (selected) {
        Button(
            modifier = Modifier.testTag(tag),
            onClick = onClick,
        ) {
            Text(label)
        }
    } else {
        TextButton(
            modifier = Modifier.testTag(tag),
            onClick = onClick,
        ) {
            Text(label)
        }
    }
}

@Composable
private fun ProductDetailScreen(appState: QuartermasterAppState, modifier: Modifier = Modifier) {
    val scope = rememberCoroutineScope()
    val product = appState.selectedCatalogueProduct
    LazyColumn(
        modifier = modifier
            .fillMaxSize()
            .padding(16.dp)
            .testTag(SmokeTag.ProductsScreen),
        verticalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        item {
            TextButton(onClick = appState::showProductList) { Text("Back to products") }
        }
        if (product == null) {
            item { StatusCard("Product unavailable", "Return to the catalogue and choose another product.") }
            return@LazyColumn
        }
        appState.productError?.let { message ->
            item { ErrorCard("Product action failed", message) }
        }
        item {
            Card {
                Column(
                    modifier = Modifier
                        .fillMaxWidth()
                        .padding(16.dp),
                    verticalArrangement = Arrangement.spacedBy(8.dp),
                ) {
                    Text(product.name, style = MaterialTheme.typography.headlineSmall)
                    Text(product.brand ?: "No brand")
                    Text("Source: ${appState.productSourceLabel(product)}")
                    Text("Family: ${product.family.value}")
                    Text("Preferred unit: ${product.preferredUnit}")
                    Text("Barcode: ${product.barcode ?: "No barcode"}")
                    Text("Image URL: ${product.imageUrl ?: "No image"}")
                    Text("Status: ${if (appState.isDeletedProduct(product)) "Deleted ${product.deletedAt}" else "Active"}")
                }
            }
        }
        item {
            Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                when {
                    appState.isManualProduct(product) && !appState.isDeletedProduct(product) -> {
                        Button(
                            modifier = Modifier.testTag(SmokeTag.ProductEditButton),
                            onClick = appState::showProductEdit,
                            enabled = appState.productActionInFlight == null,
                        ) { Text("Edit") }
                        TextButton(
                            modifier = Modifier.testTag(SmokeTag.ProductDeleteButton),
                            onClick = appState::showProductDelete,
                            enabled = appState.productActionInFlight == null,
                        ) { Text("Delete") }
                    }
                    appState.isManualProduct(product) -> {
                        Button(
                            modifier = Modifier.testTag(SmokeTag.ProductRestoreButton),
                            onClick = { scope.launch { appState.restoreSelectedProduct() } },
                            enabled = appState.productActionInFlight == null,
                        ) {
                            Text(if (appState.productActionInFlight == ProductAction.Restore) "Restoring..." else "Restore product")
                        }
                    }
                    else -> {
                        Button(
                            modifier = Modifier.testTag(SmokeTag.ProductRefreshButton),
                            onClick = { scope.launch { appState.refreshSelectedProductFromOff() } },
                            enabled = appState.productActionInFlight == null,
                        ) {
                            Text(if (appState.productActionInFlight == ProductAction.Refresh) "Refreshing..." else "Refresh from OpenFoodFacts")
                        }
                    }
                }
            }
        }
    }
}

@Composable
private fun ProductFormScreen(
    appState: QuartermasterAppState,
    product: ProductDto?,
    modifier: Modifier = Modifier,
) {
    val scope = rememberCoroutineScope()
    var fields by remember(product?.id) {
        mutableStateOf(
            product?.let(appState::productFormFields)
                ?: ProductFormFields(preferredUnit = appState.defaultProductUnitFor(UnitFamily.MASS)),
        )
    }
    val unitChoices = appState.productUnitSymbolsFor(fields.family)
    val title = if (product == null) "New product" else "Edit product"
    LazyColumn(
        modifier = modifier
            .fillMaxSize()
            .padding(16.dp)
            .testTag(SmokeTag.ProductsScreen),
        verticalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        item { Text(title, style = MaterialTheme.typography.headlineSmall) }
        appState.productError?.let { message ->
            item { ErrorCard("Product action failed", message) }
        }
        item {
            OutlinedTextField(
                value = fields.name,
                onValueChange = { fields = fields.copy(name = it) },
                label = { Text("Product name") },
                modifier = Modifier
                    .fillMaxWidth()
                    .testTag(SmokeTag.ProductNameField),
            )
        }
        item {
            OutlinedTextField(
                value = fields.brand,
                onValueChange = { fields = fields.copy(brand = it) },
                label = { Text("Brand") },
                modifier = Modifier
                    .fillMaxWidth()
                    .testTag(SmokeTag.ProductBrandField),
            )
        }
        item {
            SelectionCard(
                title = "Product family",
                options = UnitFamily.values().map { it.name to it.value },
                selected = fields.family.name,
                emptyText = "",
                onSelect = { fields = appState.productFormWithFamily(fields, UnitFamily.valueOf(it)) },
            )
        }
        item {
            SelectionCard(
                title = "Preferred unit",
                options = unitChoices.map { it to it },
                selected = fields.preferredUnit,
                emptyText = "No units are available for this product family.",
                onSelect = { fields = fields.copy(preferredUnit = it) },
            )
        }
        item {
            OutlinedTextField(
                value = fields.imageUrl,
                onValueChange = { fields = fields.copy(imageUrl = it) },
                label = { Text("Image URL") },
                modifier = Modifier
                    .fillMaxWidth()
                    .testTag(SmokeTag.ProductImageUrlField),
            )
        }
        item {
            Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                Button(
                    modifier = Modifier.testTag(SmokeTag.ProductSubmitButton),
                    onClick = {
                        scope.launch {
                            if (product == null) {
                                appState.createProduct(fields)
                            } else {
                                appState.updateSelectedProduct(fields)
                            }
                        }
                    },
                    enabled = appState.productActionInFlight == null,
                ) {
                    Text(
                        when (appState.productActionInFlight) {
                            ProductAction.Create -> "Creating..."
                            ProductAction.Update -> "Saving..."
                            else -> if (product == null) "Create product" else "Save product"
                        },
                    )
                }
                TextButton(onClick = appState::cancelProductForm) {
                    Text("Cancel")
                }
            }
        }
    }
}

@Composable
private fun ProductDeleteScreen(appState: QuartermasterAppState, modifier: Modifier = Modifier) {
    val scope = rememberCoroutineScope()
    val product = appState.selectedCatalogueProduct
    LazyColumn(
        modifier = modifier
            .fillMaxSize()
            .padding(16.dp)
            .testTag(SmokeTag.ProductsScreen),
        verticalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        item { Text("Delete product", style = MaterialTheme.typography.headlineSmall) }
        appState.productError?.let { message ->
            item { ErrorCard("Product action failed", message) }
        }
        item {
            StatusCard(
                title = product?.name ?: "Product unavailable",
                message = "Deleted manual products can be restored later from the catalogue's Deleted filter.",
            )
        }
        item {
            Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                Button(
                    modifier = Modifier.testTag(SmokeTag.ProductDeleteConfirmButton),
                    onClick = { scope.launch { appState.deleteSelectedProduct() } },
                    enabled = product != null && appState.productActionInFlight == null,
                ) {
                    Text(if (appState.productActionInFlight == ProductAction.Delete) "Deleting..." else "Delete product")
                }
                TextButton(onClick = appState::showProductDetail) {
                    Text("Cancel")
                }
            }
        }
    }
}

@Composable
private fun SettingsScreen(appState: QuartermasterAppState, modifier: Modifier = Modifier) {
    val scope = rememberCoroutineScope()
    var inviteExpiry by remember { mutableStateOf("2999-01-01T00:00:00.000Z") }
    var inviteMaxUses by remember { mutableStateOf("1") }
    var redeemInviteCode by remember { mutableStateOf(appState.pendingInviteContext?.inviteCode.orEmpty()) }
    var locationForm by remember { mutableStateOf<LocationFormFields?>(null) }
    var editingLocationId by remember { mutableStateOf<String?>(null) }
    var deletingLocation by remember { mutableStateOf<LocationDto?>(null) }

    LaunchedEffect(appState.currentHouseholdId) { appState.loadSettings() }
    LaunchedEffect(appState.pendingInviteContext) {
        if (!appState.pendingInviteContext?.inviteCode.isNullOrBlank()) {
            redeemInviteCode = appState.pendingInviteContext?.inviteCode.orEmpty()
        }
    }

    LazyColumn(
        modifier = modifier
            .fillMaxSize()
            .padding(16.dp),
        verticalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        item { Text("Settings", style = MaterialTheme.typography.headlineSmall) }
        if (appState.isSettingsRefreshing) {
            item {
                StatusCard(
                    title = "Refreshing settings",
                    message = "Quartermaster is syncing household details and invite state for this session.",
                )
            }
        }
        appState.settingsError?.let { message ->
            item { ErrorCard("Settings couldn't refresh", message) }
        }
        item {
            SectionHeader(
                title = "Current session",
                body = "Account identity, current household, and server details for this device session.",
            )
        }
        item {
            Card {
                Column(
                    modifier = Modifier
                        .fillMaxWidth()
                        .padding(16.dp),
                    verticalArrangement = Arrangement.spacedBy(6.dp),
                ) {
                    Text(appState.meOrNull?.user?.username ?: "")
                    appState.meOrNull?.user?.email?.let { Text(it) }
                    Text("Household: ${appState.meOrNull?.currentHousehold?.name ?: "None"}")
                    Text("Timezone: ${appState.meOrNull?.currentHousehold?.timezone ?: "UTC"}")
                    Text("Server: ${appState.serverUrl}")
                }
            }
        }
        item {
            SectionHeader(
                title = "Household actions",
                body = "Switch households or bring a new invite into the current account.",
            )
        }
        if (appState.hasPendingInviteHandoff) {
            item {
                InviteHandoffCard(
                    inviteCode = appState.pendingInviteCode,
                    onDismiss = appState::clearPendingInviteContext,
                )
            }
        }
        item {
            Text(
                "Switch household",
                style = MaterialTheme.typography.titleMedium,
                modifier = Modifier.testTag(SmokeTag.SwitchHouseholdHeader),
            )
        }
        items(appState.meOrNull?.households.orEmpty(), key = { it.id }) { household ->
            Card {
                Row(
                    modifier = Modifier
                        .fillMaxWidth()
                        .padding(16.dp),
                    horizontalArrangement = Arrangement.SpaceBetween,
                ) {
                    Column {
                        Text(household.name)
                        Text(household.role.name)
                    }
                    TextButton(
                        onClick = { scope.launch { appState.switchHousehold(household.id.toString()) } },
                        enabled = appState.settingsLoadState != LoadState.Loading,
                    ) {
                        Text("Switch")
                    }
                }
            }
        }
        item {
            Text("Redeem invite", style = MaterialTheme.typography.titleMedium)
        }
        item {
            OutlinedTextField(
                value = redeemInviteCode,
                onValueChange = { redeemInviteCode = it },
                label = { Text("Invite code") },
                modifier = Modifier.fillMaxWidth(),
            )
        }
        item {
            Button(
                onClick = { scope.launch { appState.redeemInvite(redeemInviteCode.trim()) } },
                enabled = appState.settingsLoadState != LoadState.Loading && redeemInviteCode.isNotBlank(),
            ) {
                Text(if (appState.settingsLoadState == LoadState.Loading) "Working..." else "Redeem invite")
            }
        }
        item {
            SectionHeader(
                title = "Locations",
                body = "Manage the pantry, fridge, and freezer places available to Inventory and Scan.",
            )
        }
        item {
            Row(
                modifier = Modifier.fillMaxWidth(),
                horizontalArrangement = Arrangement.SpaceBetween,
            ) {
                Text("Household locations", style = MaterialTheme.typography.titleMedium)
                Button(
                    modifier = Modifier.testTag(SmokeTag.LocationCreateButton),
                    onClick = {
                        editingLocationId = null
                        deletingLocation = null
                        locationForm = LocationFormFields(
                            sortOrder = (appState.sortedLocations().maxOfOrNull { it.sortOrder } ?: -1L) + 1L,
                        )
                    },
                    enabled = appState.locationActionInFlight == null,
                ) {
                    Text("New location")
                }
            }
        }
        locationForm?.let { fields ->
            item {
                LocationFormCard(
                    fields = fields,
                    isEditing = editingLocationId != null,
                    actionInFlight = appState.locationActionInFlight,
                    onFieldsChange = { locationForm = it },
                    onSubmit = {
                        locationForm?.let { currentFields ->
                            scope.launch {
                                val id = editingLocationId
                                if (id == null) {
                                    appState.createLocation(currentFields)
                                } else {
                                    appState.updateLocation(id, currentFields)
                                }
                                if (appState.settingsError == null) {
                                    locationForm = null
                                    editingLocationId = null
                                }
                            }
                        }
                    },
                    onCancel = {
                        locationForm = null
                        editingLocationId = null
                    },
                )
            }
        }
        deletingLocation?.let { location ->
            item {
                LocationDeleteCard(
                    location = location,
                    actionInFlight = appState.locationActionInFlight,
                    onConfirm = {
                        scope.launch {
                            appState.deleteLocation(location.id.toString())
                            if (appState.settingsError == null) {
                                deletingLocation = null
                            }
                        }
                    },
                    onCancel = { deletingLocation = null },
                )
            }
        }
        item {
            Text(
                "Location list",
                style = MaterialTheme.typography.titleMedium,
                modifier = Modifier.testTag(SmokeTag.LocationList),
            )
        }
        val settingsLocations = appState.sortedLocations()
        if (settingsLocations.isEmpty()) {
            item { StatusCard("No locations yet", "Create a location before adding stock to this household.") }
        } else {
            items(settingsLocations, key = { it.id }) { location ->
                LocationRow(
                    appState = appState,
                    location = location,
                    isFirst = location == settingsLocations.first(),
                    isLast = location == settingsLocations.last(),
                    onEdit = {
                        deletingLocation = null
                        editingLocationId = location.id.toString()
                        locationForm = appState.locationFormFields(location)
                    },
                    onDelete = {
                        locationForm = null
                        editingLocationId = null
                        deletingLocation = location
                    },
                    onMove = { delta -> scope.launch { appState.moveLocation(location.id.toString(), delta) } },
                )
            }
        }
        item {
            SectionHeader(
                title = "Invites",
                body = "Create a join code for another person, then verify it from the list below.",
            )
        }
        item {
            Text("Create invite", style = MaterialTheme.typography.titleMedium)
        }
        item {
            OutlinedTextField(
                value = inviteExpiry,
                onValueChange = { inviteExpiry = it },
                label = { Text("Expires at") },
                modifier = Modifier.fillMaxWidth(),
            )
        }
        item {
            OutlinedTextField(
                value = inviteMaxUses,
                onValueChange = { inviteMaxUses = it },
                label = { Text("Max uses") },
                keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Number),
                modifier = Modifier.fillMaxWidth(),
            )
        }
        item {
            Button(
                modifier = Modifier.testTag(SmokeTag.CreateInviteButton),
                onClick = {
                    scope.launch {
                        appState.createInvite(
                            expiresAt = inviteExpiry,
                            maxUses = inviteMaxUses.toLongOrNull() ?: 1L,
                        )
                    }
                },
                enabled = appState.settingsLoadState != LoadState.Loading,
            ) {
                Text("Create invite")
            }
        }
        items(appState.invites, key = { it.id }) { invite ->
            Card {
                Column(
                    modifier = Modifier
                        .fillMaxWidth()
                        .padding(16.dp),
                    verticalArrangement = Arrangement.spacedBy(4.dp),
                ) {
                    Text(invite.code)
                    Text("Uses ${invite.useCount}/${invite.maxUses}")
                }
            }
        }
        item {
            SectionHeader(
                title = "Session actions",
                body = "Sign out on this device without affecting other sessions.",
            )
        }
        item {
            Button(
                modifier = Modifier.testTag(SmokeTag.SignOutButton),
                onClick = { scope.launch { appState.logout() } },
                enabled = !appState.authActionInFlight,
            ) {
                Text("Sign out")
            }
        }
    }
}

@Composable
private fun LocationFormCard(
    fields: LocationFormFields,
    isEditing: Boolean,
    actionInFlight: LocationAction?,
    onFieldsChange: (LocationFormFields) -> Unit,
    onSubmit: () -> Unit,
    onCancel: () -> Unit,
) {
    Card {
        Column(
            modifier = Modifier
                .fillMaxWidth()
                .padding(16.dp),
            verticalArrangement = Arrangement.spacedBy(12.dp),
        ) {
            Text(if (isEditing) "Edit location" else "New location", style = MaterialTheme.typography.titleMedium)
            OutlinedTextField(
                value = fields.name,
                onValueChange = { onFieldsChange(fields.copy(name = it)) },
                label = { Text("Location name") },
                modifier = Modifier
                    .fillMaxWidth()
                    .testTag(SmokeTag.LocationNameField),
            )
            Text("Kind", style = MaterialTheme.typography.titleMedium)
            Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                LocationKindButton("Pantry", "pantry", SmokeTag.LocationKindPantry, fields.kind, onFieldsChange, fields)
                LocationKindButton("Fridge", "fridge", SmokeTag.LocationKindFridge, fields.kind, onFieldsChange, fields)
                LocationKindButton("Freezer", "freezer", SmokeTag.LocationKindFreezer, fields.kind, onFieldsChange, fields)
            }
            Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                Button(
                    modifier = Modifier.testTag(SmokeTag.LocationSubmitButton),
                    onClick = onSubmit,
                    enabled = actionInFlight == null,
                ) {
                    Text(
                        when (actionInFlight) {
                            LocationAction.Create -> "Creating..."
                            LocationAction.Update -> "Saving..."
                            else -> if (isEditing) "Save location" else "Create location"
                        },
                    )
                }
                TextButton(onClick = onCancel, enabled = actionInFlight == null) {
                    Text("Cancel")
                }
            }
        }
    }
}

@Composable
private fun LocationKindButton(
    label: String,
    kind: String,
    tag: String,
    selectedKind: String,
    onFieldsChange: (LocationFormFields) -> Unit,
    fields: LocationFormFields,
) {
    if (selectedKind == kind) {
        Button(
            modifier = Modifier.testTag(tag),
            onClick = { onFieldsChange(fields.copy(kind = kind)) },
        ) {
            Text(label)
        }
    } else {
        TextButton(
            modifier = Modifier.testTag(tag),
            onClick = { onFieldsChange(fields.copy(kind = kind)) },
        ) {
            Text(label)
        }
    }
}

@Composable
private fun LocationDeleteCard(
    location: LocationDto,
    actionInFlight: LocationAction?,
    onConfirm: () -> Unit,
    onCancel: () -> Unit,
) {
    StatusCard(
        title = "Delete ${location.name}?",
        message = "Locations with active stock cannot be deleted. Move or consume stock before deleting a location.",
    )
    Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
        Button(
            modifier = Modifier.testTag(SmokeTag.locationDeleteConfirm(location.id.toString())),
            onClick = onConfirm,
            enabled = actionInFlight == null,
        ) {
            Text(if (actionInFlight == LocationAction.Delete) "Deleting..." else "Delete location")
        }
        TextButton(onClick = onCancel, enabled = actionInFlight == null) {
            Text("Cancel")
        }
    }
}

@Composable
private fun LocationRow(
    appState: QuartermasterAppState,
    location: LocationDto,
    isFirst: Boolean,
    isLast: Boolean,
    onEdit: () -> Unit,
    onDelete: () -> Unit,
    onMove: (Int) -> Unit,
) {
    Card {
        Column(
            modifier = Modifier
                .fillMaxWidth()
                .padding(16.dp),
            verticalArrangement = Arrangement.spacedBy(8.dp),
        ) {
            Text(location.name, style = MaterialTheme.typography.titleMedium)
            Text("${location.kind} · position ${location.sortOrder}")
            Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                TextButton(
                    modifier = Modifier.testTag(SmokeTag.locationMoveUp(location.id.toString())),
                    onClick = { onMove(-1) },
                    enabled = !isFirst && appState.locationActionInFlight == null,
                ) {
                    Text("Up")
                }
                TextButton(
                    modifier = Modifier.testTag(SmokeTag.locationMoveDown(location.id.toString())),
                    onClick = { onMove(1) },
                    enabled = !isLast && appState.locationActionInFlight == null,
                ) {
                    Text("Down")
                }
                TextButton(
                    modifier = Modifier.testTag(SmokeTag.locationEdit(location.id.toString())),
                    onClick = onEdit,
                    enabled = appState.locationActionInFlight == null,
                ) {
                    Text("Edit")
                }
                TextButton(
                    modifier = Modifier.testTag(SmokeTag.locationDelete(location.id.toString())),
                    onClick = onDelete,
                    enabled = appState.locationActionInFlight == null,
                ) {
                    Text("Delete")
                }
            }
        }
    }
}

@Composable
private fun ErrorCard(title: String, message: String) {
    Card {
        Column(
            modifier = Modifier
                .fillMaxWidth()
                .padding(16.dp),
            verticalArrangement = Arrangement.spacedBy(6.dp),
        ) {
            Text(title, style = MaterialTheme.typography.titleMedium)
            Text(message)
        }
    }
}

@Composable
private fun SelectionCard(
    title: String,
    options: List<Pair<String, String>>,
    selected: String?,
    emptyText: String,
    onSelect: (String) -> Unit,
) {
    Card {
        Column(
            modifier = Modifier
                .fillMaxWidth()
                .padding(16.dp),
            verticalArrangement = Arrangement.spacedBy(8.dp),
        ) {
            Text(title, style = MaterialTheme.typography.titleMedium)
            if (options.isEmpty()) {
                Text(emptyText)
            } else {
                options.forEach { (value, label) ->
                    val isSelected = value == selected
                    Row(
                        modifier = Modifier.fillMaxWidth(),
                        horizontalArrangement = Arrangement.SpaceBetween,
                    ) {
                        Text(label)
                        if (isSelected) {
                            Text("Selected", style = MaterialTheme.typography.labelMedium)
                        } else {
                            TextButton(onClick = { onSelect(value) }) {
                                Text("Select")
                            }
                        }
                    }
                }
            }
        }
    }
}

private fun QuartermasterAppState.batchesForLocation(
    locationId: String,
    target: InventoryTarget?,
): List<StockBatchDto> = batches.filter { it.locationId.toString() == locationId }
    .sortedWith(
        compareByDescending<StockBatchDto> { it.product.id.toString() == target?.productId }
            .thenBy { isBatchDepleted(it) }
            .thenBy { it.product.name.lowercase() }
            .thenBy { it.expiresOn ?: "9999-12-31" },
    )
