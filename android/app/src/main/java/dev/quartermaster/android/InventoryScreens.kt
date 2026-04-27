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
import androidx.compose.material3.Button
import androidx.compose.material3.Card
import androidx.compose.material3.CardDefaults
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
import dev.quartermaster.android.generated.models.StockBatchDto
import dev.quartermaster.android.generated.models.StockEventDto
import kotlinx.coroutines.launch

@Composable
internal fun InventoryScreen(
    appState: QuartermasterAppState,
    modifier: Modifier = Modifier,
    onOpenBatch: (String) -> Unit = {},
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
            RouteHeader(
                title = appState.meOrNull?.currentHousehold?.name ?: "Inventory",
                subtitle = "Stock grouped by household location, with depleted batches kept for history and restore.",
            )
        }
        if (appState.isInventoryRefreshing) {
            item {
                InlineStatusCard(
                    title = "Refreshing inventory",
                    message = "Syncing stock, locations, and recent history.",
                )
            }
        }

        when {
            appState.inventoryLoadState == LoadState.Loading && !appState.hasLoadedInventoryOnce -> {
                item {
                    InlineStatusCard(
                        title = "Loading inventory",
                        message = "Fetching locations, batches, and recent stock history.",
                    )
                }
            }
            appState.inventoryError != null && !appState.hasLoadedInventoryOnce -> {
                item {
                    ErrorCard(
                        title = "Couldn't load inventory",
                        message = appState.inventoryError!!,
                        actionLabel = "Retry",
                        onAction = { scope.launch { appState.refreshInventory(force = true) } },
                    )
                }
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
            item {
                ErrorCard(
                    title = "Inventory action failed",
                    message = message,
                    actionLabel = "Refresh inventory",
                    onAction = { scope.launch { appState.refreshInventory(force = true) } },
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
                counts = appState.batchCountsForLocation(location.id.toString()),
                target = target,
                selectedBatchId = appState.selectedBatchId,
                isBatchDepleted = appState::isBatchDepleted,
                onSelectBatch = { batchId ->
                    scope.launch { appState.selectBatch(batchId) }
                    onOpenBatch(batchId)
                },
            )
        }

        if (appState.history.isNotEmpty()) {
            item {
                SectionHeader(
                    title = "Recent history",
                    body = "Latest stock changes across the household.",
                )
            }
            items(appState.history.take(10), key = { it.id }) { event ->
                Card {
                    Column(
                        modifier = Modifier
                            .fillMaxWidth()
                            .padding(16.dp),
                        verticalArrangement = Arrangement.spacedBy(4.dp),
                    ) {
                        Text(event.product.name, style = MaterialTheme.typography.titleSmall)
                        Text("${event.eventType.name.lowercase()} · ${event.quantityDelta} ${event.unit}")
                        Text(event.createdAt, style = MaterialTheme.typography.bodySmall)
                    }
                }
            }
        }
    }
}

internal fun QuartermasterAppState.batchesForLocation(
    locationId: String,
    target: InventoryTarget?,
): List<StockBatchDto> = batches.filter { it.locationId.toString() == locationId }
    .sortedWith(
        compareByDescending<StockBatchDto> { it.id.toString() == target?.batchId }
            .thenByDescending { it.product.id.toString() == target?.productId }
            .thenBy { isBatchDepleted(it) }
            .thenBy { it.product.name.lowercase() }
            .thenBy { it.expiresOn ?: "9999-12-31" },
    )

@Composable
private fun LocationInventoryCard(
    location: LocationDto,
    batches: List<StockBatchDto>,
    counts: BatchCounts,
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
            Text(
                if (batches.isEmpty()) {
                    "No active or depleted batches in this location."
                } else {
                    "${counts.active} active · ${counts.depleted} depleted"
                },
                style = MaterialTheme.typography.bodySmall,
            )
            if (batches.isEmpty()) {
                Text("No stock recorded.", style = MaterialTheme.typography.bodyMedium)
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
                            Text(batch.product.name, style = MaterialTheme.typography.titleSmall)
                            Text("${batch.quantity} ${batch.unit} · ${if (depleted) "depleted history" else "available"}")
                            Text("Expires ${batch.expiresOn ?: "not set"}", style = MaterialTheme.typography.bodySmall)
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
internal fun BatchDetailScreen(
    appState: QuartermasterAppState,
    batchId: String,
    onBack: () -> Unit,
    onEditBatch: () -> Unit,
    modifier: Modifier = Modifier,
) {
    val scope = rememberCoroutineScope()
    val batch = appState.batches.firstOrNull { it.id.toString() == batchId }

    LaunchedEffect(batchId) {
        if (appState.selectedBatchId != batchId) {
            appState.selectBatch(batchId)
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
            RouteHeader(
                title = batch?.product?.name ?: "Batch detail",
                subtitle = batch?.let { "${it.quantity} ${it.unit} in ${appState.locationNameFor(it)}" },
                backLabel = "Back to inventory",
                onBack = onBack,
            )
        }
        appState.inventoryError?.let { message ->
            item {
                ErrorCard(
                    title = "Inventory action failed",
                    message = message,
                    actionLabel = "Refresh inventory",
                    onAction = { scope.launch { appState.refreshInventory(force = true) } },
                )
            }
        }
        if (batch == null) {
            item {
                StatusCard(
                    title = "Batch unavailable",
                    message = "Return to Inventory and choose another batch. The household stock may have refreshed.",
                )
            }
        } else {
            item {
                BatchDetailCard(
                    appState = appState,
                    batch = batch,
                    onEdit = onEditBatch,
                    onConsume = { quantity -> scope.launch { appState.consumeSelectedBatch(quantity) } },
                    onDiscard = { scope.launch { appState.discardBatch(batch.id.toString()) } },
                    onRestore = { scope.launch { appState.restoreBatch(batch.id.toString()) } },
                    onClose = onBack,
                )
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
                    Text("${batch.quantity} ${batch.unit} in ${appState.locationNameFor(batch)}")
                }
                TextButton(onClick = onClose) {
                    Text("Close")
                }
            }
            BatchMetadata(batch)
            action?.let {
                InlineStatusCard(
                    title = "Working on this batch",
                    message = when (it) {
                        StockAction.LoadEvents -> "Loading the latest batch history."
                        StockAction.Update -> "Saving stock correction."
                        StockAction.Consume -> "Recording consumption."
                        StockAction.Discard -> "Discarding this batch."
                        StockAction.Restore -> "Restoring this batch."
                    },
                )
            }
            if (depleted) {
                StatusCard(
                    title = "Batch is depleted",
                    message = "Consume and edit actions are disabled. Restore is available only when the latest event allows it.",
                )
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
                    label = { Text("Use quantity (${batch.unit})") },
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
                    Text(if (action == StockAction.Consume) "Using..." else "Use stock")
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
            BatchHistory(
                events = appState.selectedBatchEvents,
                loadState = appState.selectedBatchEventLoadState,
                error = appState.selectedBatchEventError,
                onRetry = { appState.loadSelectedBatchEvents() },
            )
        }
    }
}

@Composable
private fun BatchMetadata(batch: StockBatchDto) {
    Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
        MetadataRow("Created", batch.createdAt)
        MetadataRow("Initial quantity", "${batch.initialQuantity} ${batch.unit}")
        MetadataRow("Expiry", batch.expiresOn ?: "No expiry date")
        MetadataRow("Opened", batch.openedOn ?: "Not marked opened")
        batch.note?.takeIf(String::isNotBlank)?.let {
            MetadataRow("Note", it)
        }
    }
}

@Composable
private fun BatchHistory(
    events: List<StockEventDto>,
    loadState: LoadState,
    error: String?,
    onRetry: suspend () -> Unit,
) {
    val scope = rememberCoroutineScope()
    Column(verticalArrangement = Arrangement.spacedBy(6.dp)) {
        SectionHeader(
            title = "Batch history",
            body = "Audit trail for quantity changes on this batch.",
        )
        when {
            loadState == LoadState.Loading && events.isEmpty() -> InlineStatusCard("Loading history", "Fetching the latest events.")
            error != null -> ErrorCard(
                title = "Couldn't load history",
                message = error,
                actionLabel = "Retry",
                onAction = { scope.launch { onRetry() } },
            )
            events.isEmpty() -> StatusCard("No history yet", "This batch does not have any visible events yet.")
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
internal fun StockEditScreen(
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
            RouteHeader(
                title = batch?.let { "Edit ${it.product.name}" } ?: "Edit stock",
                subtitle = "Correct quantity, location, expiry, opened date, or note for this active batch.",
            )
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
                modifier = Modifier
                    .testTag(SmokeTag.StockEditSave)
                    .fillMaxWidth(),
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
        item {
            Spacer(Modifier.height(96.dp))
        }
    }
}
