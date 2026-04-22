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
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.outlined.Bell
import androidx.compose.material.icons.outlined.Inventory2
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
import androidx.compose.ui.unit.dp
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
                            MainTab.Reminders to Pair("Reminders", Icons.Outlined.Bell),
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
                    actionLabel = "Sign in",
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
        modifier = modifier.fillMaxSize().padding(24.dp),
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
    actionLabel: String,
    modifier: Modifier = Modifier,
) {
    Column(
        modifier = modifier.fillMaxSize().padding(24.dp),
        verticalArrangement = Arrangement.Center,
    ) {
        Text(title, style = MaterialTheme.typography.headlineSmall)
        Spacer(Modifier.height(12.dp))
        Text(message, style = MaterialTheme.typography.bodyMedium)
        Spacer(Modifier.height(16.dp))
        Text(actionLabel, style = MaterialTheme.typography.labelLarge)
    }
}

@Composable
private fun OnboardingScreen(appState: QuartermasterAppState, modifier: Modifier = Modifier) {
    val scope = rememberCoroutineScope()
    var username by remember { mutableStateOf("") }
    var password by remember { mutableStateOf("") }
    var email by remember { mutableStateOf("") }
    var inviteCode by remember { mutableStateOf(appState.pendingInviteCode.orEmpty()) }
    var serverUrl by remember { mutableStateOf(appState.serverUrl) }
    var signInMode by remember { mutableStateOf(true) }

    LaunchedEffect(appState.pendingInviteCode) {
        if (!appState.pendingInviteCode.isNullOrBlank()) {
            inviteCode = appState.pendingInviteCode.orEmpty()
            signInMode = false
        }
    }

    LazyColumn(
        modifier = modifier.fillMaxSize().padding(16.dp),
        verticalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        item {
            Text("Know what’s in your kitchen.", style = MaterialTheme.typography.headlineSmall)
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
                enabled = !appState.isBusy && username.isNotBlank() && password.length >= 8,
            ) {
                Text(if (signInMode) "Sign in" else "Create household")
            }
        }
    }
}

@Composable
private fun NoHouseholdScreen(appState: QuartermasterAppState, modifier: Modifier = Modifier) {
    val scope = rememberCoroutineScope()
    var householdName by remember { mutableStateOf("") }
    var timezone by remember { mutableStateOf("UTC") }
    var inviteCode by remember { mutableStateOf(appState.pendingInviteCode.orEmpty()) }

    LazyColumn(
        modifier = modifier.fillMaxSize().padding(16.dp),
        verticalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        item {
            Text("No household selected", style = MaterialTheme.typography.headlineSmall)
        }
        item {
            Text("Create a household, redeem an invite, or switch into an existing membership.")
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
                onClick = {
                    scope.launch {
                        appState.createHousehold(householdName, timezone)
                    }
                },
                enabled = !appState.isBusy && householdName.isNotBlank(),
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
                onClick = {
                    scope.launch { appState.redeemInvite(inviteCode) }
                },
                enabled = !appState.isBusy && inviteCode.isNotBlank(),
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
                    modifier = Modifier.fillMaxWidth().padding(16.dp),
                    horizontalArrangement = Arrangement.SpaceBetween,
                ) {
                    Column {
                        Text(household.name)
                        Text(household.role.name)
                    }
                    TextButton(onClick = { scope.launch { appState.switchHousehold(household.id) } }) {
                        Text("Switch")
                    }
                }
            }
        }
    }
}

@Composable
private fun InventoryScreen(appState: QuartermasterAppState, modifier: Modifier = Modifier) {
    LaunchedEffect(Unit) { appState.refreshInventory() }

    LazyColumn(
        modifier = modifier.fillMaxSize().padding(16.dp),
        verticalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        item {
            Text(
                appState.meOrNull?.currentHousehold?.name ?: "Inventory",
                style = MaterialTheme.typography.headlineSmall,
            )
        }
        items(appState.locations, key = { it.id }) { location ->
            Card {
                Column(Modifier.fillMaxWidth().padding(16.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                    Text(location.name, style = MaterialTheme.typography.titleMedium)
                    val locationBatches = appState.batches.filter { it.locationId == location.id }
                    if (locationBatches.isEmpty()) {
                        Text("Nothing here yet.")
                    } else {
                        locationBatches.forEach { batch ->
                            Text("${batch.product.name} · ${batch.quantity} ${batch.unit}")
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
    LaunchedEffect(Unit) { appState.refreshReminders() }

    LazyColumn(
        modifier = modifier.fillMaxSize().padding(16.dp),
        verticalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        item { Text("Reminders", style = MaterialTheme.typography.headlineSmall) }
        items(appState.reminders, key = { it.id }) { reminder ->
            Card {
                Column(Modifier.fillMaxWidth().padding(16.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                    Text(reminder.title, style = MaterialTheme.typography.titleMedium)
                    Text(reminder.body)
                    Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                        Button(onClick = { scope.launch { appState.openReminder(reminder) } }) {
                            Text("Open")
                        }
                        TextButton(onClick = { scope.launch { appState.acknowledgeReminder(reminder.id) } }) {
                            Text("Acknowledge")
                        }
                    }
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
    var unit by remember { mutableStateOf("g") }
    var expiresOn by remember { mutableStateOf("") }
    var note by remember { mutableStateOf("") }
    val firstLocation = appState.locations.firstOrNull()?.id

    LaunchedEffect(Unit) {
        appState.refreshInventory()
    }

    LazyColumn(
        modifier = modifier.fillMaxSize().padding(16.dp),
        verticalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        item { Text("Scan & add stock", style = MaterialTheme.typography.headlineSmall) }
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
                onClick = { scope.launch { appState.lookupBarcode(barcode) } },
                enabled = barcode.isNotBlank() && !appState.isBusy,
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
                onClick = { scope.launch { appState.searchProducts(query) } },
                enabled = query.isNotBlank() && !appState.isBusy,
            ) {
                Text("Search")
            }
        }
        items(appState.searchResults, key = { it.id }) { product ->
            Card {
                Row(
                    modifier = Modifier.fillMaxWidth().padding(16.dp),
                    horizontalArrangement = Arrangement.SpaceBetween,
                ) {
                    Column {
                        Text(product.name)
                        Text(product.family.name)
                    }
                    TextButton(onClick = { appState.selectProduct(product) }) { Text("Use") }
                }
            }
        }
        appState.selectedProduct?.let { product ->
            item {
                Text("Add ${product.name}", style = MaterialTheme.typography.titleMedium)
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
                    value = unit,
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
                                    productId = product.id,
                                    locationId = locationId,
                                    quantity = quantity,
                                    unit = unit,
                                    expiresOn = expiresOn.takeIf(String::isNotBlank),
                                    note = note.takeIf(String::isNotBlank),
                                )
                            }
                        }
                    },
                    enabled = !appState.isBusy && quantity.isNotBlank() && firstLocation != null,
                ) {
                    Text("Add stock")
                }
            }
        }
        item {
            Text("Recent history", style = MaterialTheme.typography.titleMedium)
        }
        items(appState.history, key = { it.id }) { event ->
            Card {
                Column(Modifier.fillMaxWidth().padding(16.dp), verticalArrangement = Arrangement.spacedBy(4.dp)) {
                    Text("${event.product.name} · ${event.eventType.name}")
                    Text("${event.quantityDelta} ${event.batchUnit}")
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

    LaunchedEffect(Unit) { appState.loadSettings() }

    LazyColumn(
        modifier = modifier.fillMaxSize().padding(16.dp),
        verticalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        item { Text("Settings", style = MaterialTheme.typography.headlineSmall) }
        item {
            Card {
                Column(Modifier.fillMaxWidth().padding(16.dp), verticalArrangement = Arrangement.spacedBy(6.dp)) {
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
                    modifier = Modifier.fillMaxWidth().padding(16.dp),
                    horizontalArrangement = Arrangement.SpaceBetween,
                ) {
                    Column {
                        Text(household.name)
                        Text(household.role.name)
                    }
                    TextButton(onClick = { scope.launch { appState.switchHousehold(household.id) } }) {
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
                enabled = !appState.isBusy,
            ) {
                Text("Create invite")
            }
        }
        items(appState.invites, key = { it.id }) { invite ->
            Card {
                Column(Modifier.fillMaxWidth().padding(16.dp), verticalArrangement = Arrangement.spacedBy(4.dp)) {
                    Text(invite.code)
                    Text("Uses ${invite.useCount}/${invite.maxUses}")
                }
            }
        }
        item {
            Button(
                onClick = { scope.launch { appState.logout() } },
                enabled = !appState.isBusy,
            ) {
                Text("Sign out")
            }
        }
    }
}
