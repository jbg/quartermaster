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
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.unit.dp
import dev.quartermaster.android.generated.models.LocationDto
import dev.quartermaster.android.generated.models.ProductDto
import kotlinx.coroutines.launch

@Composable
internal fun ScanScreen(
    appState: QuartermasterAppState,
    modifier: Modifier = Modifier,
    onCreateProduct: () -> Unit = {},
) {
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
                onClick = onCreateProduct,
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
