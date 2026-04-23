package dev.quartermaster.android

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
import androidx.compose.material.icons.Icons
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
import androidx.compose.ui.ExperimentalComposeUiApi
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.testTagsAsResourceId
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.ui.unit.dp
import dev.quartermaster.android.generated.models.LocationDto
import dev.quartermaster.android.generated.models.ProductDto
import dev.quartermaster.android.generated.models.ReminderDto
import dev.quartermaster.android.generated.models.StockBatchDto
import kotlinx.coroutines.launch

private object SmokeTag {
    const val OnboardingScreen = "smoke-onboarding-screen"
    const val InventoryScreen = "smoke-inventory-screen"
    const val ReminderScreen = "smoke-reminder-screen"
    const val SettingsScreen = "smoke-settings-screen"
    const val ServerUrlField = "smoke-server-url-field"
    const val UsernameField = "smoke-username-field"
    const val PasswordField = "smoke-password-field"
    const val SignInButton = "smoke-sign-in-button"
    const val RemindersTab = "smoke-tab-reminders"
    const val SettingsTab = "smoke-tab-settings"
    const val ReminderOpenedBanner = "smoke-reminder-opened-banner"
    const val ReminderOpenedDismissButton = "smoke-reminder-opened-dismiss"
    const val InviteHandoffCard = "smoke-invite-handoff-card"
    const val SwitchHouseholdHeader = "smoke-switch-household-header"
    const val SignOutButton = "smoke-sign-out-button"
    const val CreateInviteButton = "smoke-create-invite-button"

    fun reminderCard(id: String) = "smoke-reminder-card-$id"
    fun reminderAckButton(id: String) = "smoke-reminder-ack-$id"
    fun reminderOpenButton(id: String) = "smoke-reminder-open-$id"
    fun inviteCode(code: String) = "smoke-invite-code-$code"
    fun reminderTarget(batchId: String) = "smoke-reminder-target-$batchId"
}

@OptIn(ExperimentalMaterial3Api::class, ExperimentalComposeUiApi::class)
@Composable
fun QuartermasterApp(appState: QuartermasterAppState) {
    val snackbarHostState = remember { SnackbarHostState() }

    LaunchedEffect(appState.lastError) {
        appState.lastError?.let { snackbarHostState.showSnackbar(it) }
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
                            MainTab.Reminders to Pair("Reminders", Icons.Outlined.Notifications),
                            MainTab.Scan to Pair("Scan", Icons.Outlined.QrCodeScanner),
                            MainTab.Settings to Pair("Settings", Icons.Outlined.Settings),
                        ).forEach { (tab, labelIcon) ->
                            NavigationBarItem(
                                modifier = Modifier.testTag(
                                    when (tab) {
                                        MainTab.Reminders -> SmokeTag.RemindersTab
                                        MainTab.Settings -> SmokeTag.SettingsTab
                                        else -> "main-tab-${tab.name.lowercase()}"
                                    }
                                ),
                                selected = appState.selectedTab == tab,
                                onClick = { appState.selectedTab = tab },
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
                        when (appState.selectedTab) {
                            MainTab.Inventory -> InventoryScreen(appState, Modifier.padding(padding))
                            MainTab.Reminders -> ReminderScreen(appState, Modifier.padding(padding))
                            MainTab.Scan -> ScanScreen(appState, Modifier.padding(padding))
                            MainTab.Settings -> SettingsScreen(appState, Modifier.padding(padding))
                        }
                    }
            }
        }
    }
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
                    ?: Modifier
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
private fun InventoryScreen(appState: QuartermasterAppState, modifier: Modifier = Modifier) {
    LaunchedEffect(appState.currentHouseholdId) {
        appState.refreshInventory(force = true)
    }

    LazyColumn(
        modifier = modifier
            .testTag(SmokeTag.SettingsScreen)
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
                    modifier = Modifier.testTag(SmokeTag.ReminderOpenedBanner)
                )
            }
            item {
                TextButton(
                    modifier = Modifier.testTag(SmokeTag.ReminderOpenedDismissButton),
                    onClick = { appState.clearInventoryTarget() }
                ) {
                    Text("Dismiss")
                }
            }
        }

        val target = appState.pendingInventoryTarget
        val prioritizedLocations = appState.locations.sortedWith(
            compareByDescending<LocationDto> { it.id.toString() == target?.locationId }.thenBy { it.name.lowercase() }
        )

        items(prioritizedLocations, key = { it.id }) { location ->
            LocationInventoryCard(
                location = location,
                batches = appState.batchesForLocation(location.id.toString(), target),
                target = target,
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
                    val isTargetBatch =
                        batch.id.toString() == target?.batchId ||
                            (batch.product.id.toString() == target?.productId && batch.locationId.toString() == target.locationId)
                    Card(
                        colors = CardDefaults.cardColors(
                            containerColor = if (isTargetBatch) {
                                MaterialTheme.colorScheme.secondaryContainer
                            } else {
                                MaterialTheme.colorScheme.surfaceVariant
                            }
                        )
                    ) {
                        Column(
                            modifier = Modifier
                                .fillMaxWidth()
                                .padding(12.dp),
                            verticalArrangement = Arrangement.spacedBy(2.dp),
                        ) {
                            Text("${batch.product.name} · ${batch.quantity} ${batch.unit}")
                            batch.expiresOn?.let { Text("Expires $it") }
                            if (isTargetBatch) {
                                Text(
                                    "Reminder target",
                                    style = MaterialTheme.typography.labelMedium,
                                    modifier = Modifier.testTag(SmokeTag.reminderTarget(batch.id.toString()))
                                )
                            }
                        }
                    }
                }
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
                    enabled = action == null
                ) {
                    Text(if (action == ReminderAction.Open) "Opening..." else "Open")
                }
                TextButton(
                    modifier = Modifier.testTag(SmokeTag.reminderAckButton(reminder.id.toString())),
                    onClick = onAcknowledge,
                    enabled = action == null
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
private fun SettingsScreen(appState: QuartermasterAppState, modifier: Modifier = Modifier) {
    val scope = rememberCoroutineScope()
    var inviteExpiry by remember { mutableStateOf("2999-01-01T00:00:00.000Z") }
    var inviteMaxUses by remember { mutableStateOf("1") }
    var redeemInviteCode by remember { mutableStateOf(appState.pendingInviteContext?.inviteCode.orEmpty()) }

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
): List<StockBatchDto> {
    return batches.filter { it.locationId.toString() == locationId }
        .sortedWith(
            compareByDescending<StockBatchDto> { it.product.id.toString() == target?.productId }
                .thenBy { it.product.name.lowercase() }
                .thenBy { it.expiresOn ?: "9999-12-31" }
        )
}
