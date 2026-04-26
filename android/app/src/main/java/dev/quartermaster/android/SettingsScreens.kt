package dev.quartermaster.android

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material3.Button
import androidx.compose.material3.Card
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.unit.dp
import dev.quartermaster.android.generated.models.LocationDto
import kotlinx.coroutines.launch

@Composable
internal fun SettingsScreen(
    appState: QuartermasterAppState,
    modifier: Modifier = Modifier,
    onCreateLocation: () -> Unit = {},
    onEditLocation: (String) -> Unit = {},
    onDeleteLocation: (String) -> Unit = {},
) {
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
            .testTag(SmokeTag.SettingsScreen)
            .fillMaxSize()
            .padding(16.dp),
        verticalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        item {
            RouteHeader(
                title = "Settings",
                subtitle = "Household, invite, location, and session controls for this device.",
            )
        }
        if (appState.isSettingsRefreshing) {
            item {
                InlineStatusCard(
                    title = "Refreshing settings",
                    message = "Syncing household details and invite state.",
                )
            }
        }
        appState.settingsError?.let { message ->
            item {
                ErrorCard(
                    title = "Settings couldn't refresh",
                    message = message,
                    actionLabel = "Retry",
                    onAction = { scope.launch { appState.loadSettings() } },
                )
            }
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
                    MetadataRow("Username", appState.meOrNull?.user?.username ?: "Unknown")
                    appState.meOrNull?.user?.email?.let { MetadataRow("Email", it) }
                    MetadataRow("Household", appState.meOrNull?.currentHousehold?.name ?: "None")
                    MetadataRow("Timezone", appState.meOrNull?.currentHousehold?.timezone ?: "UTC")
                    MetadataRow("Server", appState.serverUrl)
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
            SectionHeader(
                title = "Switch household",
                body = "Changing household only affects this signed-in session.",
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
                        Text(household.name, style = MaterialTheme.typography.titleSmall)
                        Text(household.role.name, style = MaterialTheme.typography.bodySmall)
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
            SectionHeader(
                title = "Redeem invite",
                body = "Join another household with an invite code without losing this session.",
            )
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
                SectionHeader(
                    title = "Household locations",
                    body = "Order controls how locations appear in Inventory and Scan.",
                    modifier = Modifier.weight(1f),
                )
                Button(
                    modifier = Modifier.testTag(SmokeTag.LocationCreateButton),
                    onClick = onCreateLocation,
                    enabled = appState.locationActionInFlight == null,
                ) {
                    Text("New location")
                }
            }
        }
        val settingsLocations = appState.sortedLocations()
        item {
            SectionHeader(
                title = "Location list",
                body = "${settingsLocations.size} ${if (settingsLocations.size == 1) "location" else "locations"} configured.",
                modifier = Modifier.testTag(SmokeTag.LocationList),
            )
        }
        if (settingsLocations.isEmpty()) {
            item { StatusCard("No locations yet", "Create a location before adding stock to this household.") }
        } else {
            items(settingsLocations, key = { it.id }) { location ->
                LocationRow(
                    appState = appState,
                    location = location,
                    isFirst = location == settingsLocations.first(),
                    isLast = location == settingsLocations.last(),
                    onEdit = { onEditLocation(location.id.toString()) },
                    onDelete = { onDeleteLocation(location.id.toString()) },
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
            SectionHeader(
                title = "Create invite",
                body = "Generate a join code for another household member.",
            )
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
                    Text(invite.code, style = MaterialTheme.typography.titleMedium)
                    Text("Uses ${invite.useCount}/${invite.maxUses}")
                    Text("Expires ${invite.expiresAt}", style = MaterialTheme.typography.bodySmall)
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
internal fun LocationFormScreen(
    appState: QuartermasterAppState,
    locationId: String?,
    onDone: () -> Unit,
    onCancel: () -> Unit,
    modifier: Modifier = Modifier,
) {
    val scope = rememberCoroutineScope()
    val location = locationId?.let { id -> appState.locations.firstOrNull { it.id.toString() == id } }
    var fields by remember(locationId, location?.id) {
        mutableStateOf(
            location?.let(appState::locationFormFields)
                ?: LocationFormFields(
                    sortOrder = (appState.sortedLocations().maxOfOrNull { it.sortOrder } ?: -1L) + 1L,
                ),
        )
    }

    LazyColumn(
        modifier = modifier
            .fillMaxSize()
            .padding(16.dp),
        verticalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        item {
            RouteHeader(
                title = if (locationId == null) "New location" else "Edit location",
                subtitle = "Locations are shared by Inventory and Scan.",
                backLabel = "Back to settings",
                onBack = onCancel,
            )
        }
        appState.settingsError?.let { message ->
            item { ErrorCard("Location action failed", message) }
        }
        if (locationId != null && location == null) {
            item { StatusCard("Location unavailable", "Return to Settings and choose another location.") }
        } else {
            item {
                LocationFormCard(
                    fields = fields,
                    isEditing = locationId != null,
                    actionInFlight = appState.locationActionInFlight,
                    onFieldsChange = { fields = it },
                    onSubmit = {
                        scope.launch {
                            val saved = if (locationId == null) {
                                appState.createLocation(fields)
                            } else {
                                appState.updateLocation(locationId, fields)
                            }
                            if (saved) {
                                onDone()
                            }
                        }
                    },
                    onCancel = onCancel,
                )
            }
        }
    }
}

@Composable
internal fun LocationDeleteScreen(
    appState: QuartermasterAppState,
    locationId: String,
    onDone: () -> Unit,
    onCancel: () -> Unit,
    modifier: Modifier = Modifier,
) {
    val scope = rememberCoroutineScope()
    val location = appState.locations.firstOrNull { it.id.toString() == locationId }

    LazyColumn(
        modifier = modifier
            .fillMaxSize()
            .padding(16.dp),
        verticalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        item {
            RouteHeader(
                title = "Delete location",
                subtitle = location?.name ?: "Choose another location from Settings if this one is unavailable.",
                backLabel = "Back to settings",
                onBack = onCancel,
            )
        }
        appState.settingsError?.let { message ->
            item { ErrorCard("Location action failed", message) }
        }
        if (location == null) {
            item { StatusCard("Location unavailable", "Return to Settings and choose another location.") }
        } else {
            item {
                LocationDeleteCard(
                    location = location,
                    actionInFlight = appState.locationActionInFlight,
                    onConfirm = {
                        scope.launch {
                            if (appState.deleteLocation(location.id.toString())) {
                                onDone()
                            }
                        }
                    },
                    onCancel = onCancel,
                )
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
            SectionHeader(
                title = if (isEditing) "Location details" else "Location details",
                body = "Use a short name and a kind that matches where stock is stored.",
            )
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
    Card {
        Column(
            modifier = Modifier
                .fillMaxWidth()
                .padding(16.dp),
            verticalArrangement = Arrangement.spacedBy(12.dp),
        ) {
            SectionHeader(
                title = "Delete ${location.name}?",
                body = "Locations with active stock cannot be deleted. Move or consume stock before deleting a location.",
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
            Text("${location.kind} · position ${location.sortOrder}", style = MaterialTheme.typography.bodySmall)
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
