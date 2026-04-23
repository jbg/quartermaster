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
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.ui.unit.dp
import dev.quartermaster.android.generated.models.LocationDto
import dev.quartermaster.android.generated.models.ProductDto
import dev.quartermaster.android.generated.models.ReminderDto
import dev.quartermaster.android.generated.models.StockBatchDto
import kotlinx.coroutines.launch

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun QuartermasterApp(appState: QuartermasterAppState) {
    val snackbarHostState = remember { SnackbarHostState() }

    LaunchedEffect(appState.lastError) {
        appState.lastError?.let { snackbarHostState.showSnackbar(it) }
    }

    MaterialTheme {
        Scaffold(
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
            .fillMaxSize()
            .padding(16.dp),
        verticalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        item {
            Text("Know what’s in your kitchen.", style = MaterialTheme.typography.headlineSmall)
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
                modifier = Modifier.fillMaxWidth(),
            )
        }
        item {
            OutlinedTextField(
                value = username,
                onValueChange = { username = it },
                label = { Text("Username") },
                modifier = Modifier.fillMaxWidth(),
            )
        }
        item {
            OutlinedTextField(
                value = password,
                onValueChange = { password = it },
                label = { Text("Password") },
                modifier = Modifier.fillMaxWidth(),
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
        appState.settingsError?.let { message ->
            item { ErrorCard("Household action failed", message) }
        }
        item {
            OutlinedTextField(
                value = householdName,
                onValueChange = { householdName = it },
                label = { Text("Household name") },
                modifier = Modifier.fillMaxWidth(),
            )
        }
        item {
            OutlinedTextField(
                value = timezone,
                onValueChange = { timezone = it },
                label = { Text("Timezone") },
                modifier = Modifier.fillMaxWidth(),
            )
        }
        item {
            Button(
                onClick = { scope.launch { appState.createHousehold(householdName, timezone) } },
                enabled = appState.settingsLoadState != LoadState.Loading && householdName.isNotBlank(),
            ) {
                Text("Create household")
            }
        }
        item {
            OutlinedTextField(
                value = inviteCode,
                onValueChange = { inviteCode = it },
                label = { Text("Invite code") },
                modifier = Modifier.fillMaxWidth(),
            )
        }
        item {
            Button(
                onClick = { scope.launch { appState.redeemInvite(inviteCode) } },
                enabled = appState.settingsLoadState != LoadState.Loading && inviteCode.isNotBlank(),
            ) {
                Text("Redeem invite")
            }
        }
        item {
            Text("Available households", style = MaterialTheme.typography.titleMedium)
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

        when {
            appState.inventoryLoadState == LoadState.Loading && appState.batches.isEmpty() -> {
                item { Text("Loading inventory…") }
            }
            appState.inventoryError != null && appState.batches.isEmpty() -> {
                item { ErrorCard("Couldn't load inventory", appState.inventoryError!!) }
            }
            appState.locations.isEmpty() -> {
                item { Text("No locations yet.") }
            }
        }

        appState.pendingInventoryTarget?.let { target ->
            val product = appState.batches.firstOrNull { it.product.id.toString() == target.productId }?.product
            val location = appState.locations.firstOrNull { it.id.toString() == target.locationId }
            item {
                Card {
                    Column(
                        modifier = Modifier
                            .fillMaxWidth()
                            .padding(16.dp),
                        verticalArrangement = Arrangement.spacedBy(6.dp),
                    ) {
                        Text("Opened from reminder", style = MaterialTheme.typography.titleMedium)
                        Text(
                            when {
                                product != null && location != null -> "${product.name} in ${location.name}"
                                product != null -> product.name
                                else -> "Looking up the related stock..."
                            }
                        )
                        TextButton(onClick = { appState.clearInventoryTarget() }) {
                            Text("Dismiss")
                        }
                    }
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
) {
    Card {
        Column(
            modifier = Modifier
                .fillMaxWidth()
                .padding(16.dp),
            verticalArrangement = Arrangement.spacedBy(8.dp),
        ) {
            Text(location.name, style = MaterialTheme.typography.titleMedium)
            if (batches.isEmpty()) {
                Text("Nothing here yet.")
            } else {
                batches.forEach { batch ->
                    Text("${batch.product.name} · ${batch.quantity} ${batch.unit}")
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
            .fillMaxSize()
            .padding(16.dp),
        verticalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        item { Text("Reminders", style = MaterialTheme.typography.headlineSmall) }

        when {
            appState.remindersLoadState == LoadState.Loading && appState.reminders.isEmpty() -> {
                item { Text("Loading reminders…") }
            }
            appState.reminderError != null && appState.reminders.isEmpty() -> {
                item { ErrorCard("Couldn't load reminders", appState.reminderError!!) }
            }
            appState.reminders.isEmpty() -> {
                item { Text("No due reminders right now.") }
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
                .fillMaxWidth()
                .padding(16.dp),
            verticalArrangement = Arrangement.spacedBy(8.dp),
        ) {
            Text(reminder.title, style = MaterialTheme.typography.titleMedium)
            Text(reminder.body)
            Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                Button(onClick = onOpen, enabled = action == null) {
                    Text(if (action == ReminderAction.Open) "Opening..." else "Open")
                }
                TextButton(onClick = onAcknowledge, enabled = action == null) {
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
    var expiresOn by remember { mutableStateOf("") }
    var note by remember { mutableStateOf("") }

    LaunchedEffect(appState.currentHouseholdId) {
        appState.refreshInventory(force = appState.locations.isEmpty())
    }

    val selectedProduct = appState.selectedProduct
    val firstLocation = appState.locations.firstOrNull()?.id
    val defaultUnit = selectedProduct?.let(appState::defaultUnitSymbolFor)
    val selectedUnit = unit.ifBlank { defaultUnit.orEmpty() }

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
        item {
            OutlinedTextField(
                value = barcode,
                onValueChange = { barcode = it },
                label = { Text("Barcode") },
                keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Number),
                modifier = Modifier.fillMaxWidth(),
            )
        }
        item {
            Button(
                onClick = { scope.launch { appState.lookupBarcode(barcode.trim()) } },
                enabled = barcode.isNotBlank(),
            ) {
                Text("Look up barcode")
            }
        }
        item {
            OutlinedTextField(
                value = query,
                onValueChange = { query = it },
                label = { Text("Search products") },
                modifier = Modifier.fillMaxWidth(),
            )
        }
        item {
            Button(
                onClick = { scope.launch { appState.searchProducts(query.trim()) } },
                enabled = query.isNotBlank(),
            ) {
                Text("Search")
            }
        }
        items(appState.searchResults, key = { it.id }) { product ->
            ProductSearchResultCard(product) { appState.selectProduct(product) }
        }
        if (selectedProduct != null) {
            item {
                Text("Add ${selectedProduct.name}", style = MaterialTheme.typography.titleMedium)
            }
            item {
                OutlinedTextField(
                    value = quantity,
                    onValueChange = { quantity = it },
                    label = { Text("Quantity") },
                    modifier = Modifier.fillMaxWidth(),
                )
            }
            item {
                OutlinedTextField(
                    value = selectedUnit,
                    onValueChange = { unit = it },
                    label = { Text("Unit") },
                    modifier = Modifier.fillMaxWidth(),
                )
            }
            item {
                OutlinedTextField(
                    value = expiresOn,
                    onValueChange = { expiresOn = it },
                    label = { Text("Expires on (YYYY-MM-DD)") },
                    modifier = Modifier.fillMaxWidth(),
                )
            }
            item {
                OutlinedTextField(
                    value = note,
                    onValueChange = { note = it },
                    label = { Text("Note") },
                    modifier = Modifier.fillMaxWidth(),
                )
            }
            item {
                Button(
                    onClick = {
                        firstLocation?.let { locationId ->
                            scope.launch {
                                appState.addStock(
                                    productId = selectedProduct.id.toString(),
                                    locationId = locationId.toString(),
                                    quantity = quantity.trim(),
                                    unit = selectedUnit.trim(),
                                    expiresOn = expiresOn.takeIf(String::isNotBlank),
                                    note = note.takeIf(String::isNotBlank),
                                )
                                quantity = ""
                                unit = ""
                                expiresOn = ""
                                note = ""
                            }
                        }
                    },
                    enabled = quantity.isNotBlank() && selectedUnit.isNotBlank() && firstLocation != null,
                ) {
                    Text("Add stock")
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

    LaunchedEffect(appState.currentHouseholdId) { appState.loadSettings() }

    LazyColumn(
        modifier = modifier
            .fillMaxSize()
            .padding(16.dp),
        verticalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        item { Text("Settings", style = MaterialTheme.typography.headlineSmall) }
        appState.settingsError?.let { message ->
            item { ErrorCard("Settings couldn't refresh", message) }
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
            Text("Switch household", style = MaterialTheme.typography.titleMedium)
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
            Button(
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

private fun QuartermasterAppState.defaultUnitSymbolFor(product: ProductDto): String? {
    return units.firstOrNull { it.family == product.family }?.code
}
