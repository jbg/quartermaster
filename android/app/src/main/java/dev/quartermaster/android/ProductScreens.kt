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
import dev.quartermaster.android.generated.models.ProductDto
import dev.quartermaster.android.generated.models.UnitFamily
import kotlinx.coroutines.launch

@Composable
internal fun ProductListScreen(
    appState: QuartermasterAppState,
    modifier: Modifier = Modifier,
    onCreateProduct: () -> Unit = {},
    onOpenProduct: (String) -> Unit = {},
) {
    LaunchedEffect(appState.currentHouseholdId) {
        if (!appState.hasLoadedProductsOnce) {
            appState.refreshProducts(force = true)
        }
    }

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
            RouteHeader(
                title = "Products",
                subtitle = "Household catalogue for manual products and cached barcode lookups.",
                action = {
                    Button(
                        modifier = Modifier.testTag(SmokeTag.ProductCreateButton),
                        onClick = onCreateProduct,
                        enabled = appState.productActionInFlight == null,
                    ) {
                        Text("New product")
                    }
                },
            )
        }
        if (appState.isProductsRefreshing) {
            item { InlineStatusCard("Refreshing products", "Syncing the household catalogue.") }
        }
        appState.productError?.let { message ->
            item {
                ErrorCard(
                    title = "Product action failed",
                    message = message,
                    actionLabel = "Refresh products",
                    onAction = { scope.launch { appState.refreshProducts(force = true) } },
                )
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
                    SectionHeader(
                        title = "Find products",
                        body = "Filter the catalogue or look up a barcode before opening a detail page.",
                    )
                    OutlinedTextField(
                        value = query,
                        onValueChange = { query = it },
                        label = { Text("Search products") },
                        modifier = Modifier
                            .fillMaxWidth()
                            .testTag(SmokeTag.ProductSearchField),
                    )
                    Text("Include", style = MaterialTheme.typography.titleSmall)
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
                        onClick = {
                            scope.launch {
                                appState.lookupProductBarcode(barcode.trim())?.let { product ->
                                    onOpenProduct(product.id.toString())
                                }
                            }
                        },
                        enabled = barcode.isNotBlank() && appState.productActionInFlight == null,
                    ) {
                        Text(if (appState.productActionInFlight == ProductAction.BarcodeLookup) "Looking up..." else "Look up barcode")
                    }
                }
            }
        }
        val products = appState.visibleProducts()
        if (!appState.hasLoadedProductsOnce && appState.productLoadState == LoadState.Loading) {
            item { InlineStatusCard("Loading products", "Fetching catalogue entries and unit metadata.") }
        } else if (products.isEmpty()) {
            item {
                StatusCard(
                    title = "No products found",
                    message = "Create a manual product, change the filter, or look up a barcode to add one to the catalogue.",
                )
            }
        } else {
            item {
                SectionHeader(
                    title = "Catalogue",
                    body = "${products.size} ${if (products.size == 1) "product" else "products"} shown.",
                    modifier = Modifier.testTag(SmokeTag.ProductList),
                )
            }
            items(products, key = { it.id }) { product ->
                ProductCatalogueRow(appState, product) {
                    onOpenProduct(product.id.toString())
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
            Text(product.brand ?: "No brand")
            Text("${product.family.value} · preferred ${product.preferredUnit}")
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
internal fun ProductDetailScreen(
    appState: QuartermasterAppState,
    productId: String,
    modifier: Modifier = Modifier,
    onBack: () -> Unit = {},
    onEdit: () -> Unit = {},
    onDelete: () -> Unit = {},
) {
    val scope = rememberCoroutineScope()
    val product = appState.selectedCatalogueProduct

    LaunchedEffect(productId) {
        if (product?.id?.toString() != productId) {
            appState.openProduct(productId)
        } else {
            appState.prepareProductDetail()
        }
    }

    LazyColumn(
        modifier = modifier
            .fillMaxSize()
            .padding(16.dp)
            .testTag(SmokeTag.ProductsScreen),
        verticalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        item {
            RouteHeader(
                title = product?.name ?: "Product detail",
                subtitle = product?.let { appState.productSourceLabel(it) },
                backLabel = "Back to products",
                onBack = onBack,
            )
        }
        if (product == null) {
            item {
                StatusCard(
                    title = if (appState.productActionInFlight == ProductAction.LoadDetail) "Loading product" else "Product unavailable",
                    message = "Return to the catalogue and choose another product if this one does not load.",
                )
            }
            return@LazyColumn
        }
        appState.productError?.let { message ->
            item {
                ErrorCard(
                    title = "Product action failed",
                    message = message,
                    actionLabel = "Retry detail",
                    onAction = { scope.launch { appState.openProduct(productId) } },
                )
            }
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
                    MetadataRow("Brand", product.brand ?: "No brand")
                    MetadataRow("Source", appState.productSourceLabel(product))
                    MetadataRow("Family", product.family.value)
                    MetadataRow("Preferred unit", product.preferredUnit)
                    MetadataRow("Barcode", product.barcode ?: "No barcode")
                    MetadataRow("Image URL", product.imageUrl ?: "No image")
                    MetadataRow("Status", if (appState.isDeletedProduct(product)) "Deleted ${product.deletedAt}" else "Active")
                }
            }
        }
        item {
            Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                when {
                    appState.isManualProduct(product) && !appState.isDeletedProduct(product) -> {
                        Button(
                            modifier = Modifier.testTag(SmokeTag.ProductEditButton),
                            onClick = onEdit,
                            enabled = appState.productActionInFlight == null,
                        ) { Text("Edit") }
                        TextButton(
                            modifier = Modifier.testTag(SmokeTag.ProductDeleteButton),
                            onClick = onDelete,
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
internal fun ProductEditScreen(
    appState: QuartermasterAppState,
    productId: String,
    modifier: Modifier = Modifier,
    onDone: (ProductDto) -> Unit = {},
    onCancel: () -> Unit = {},
) {
    val product = appState.selectedCatalogueProduct?.takeIf { it.id.toString() == productId }

    LaunchedEffect(productId) {
        if (product == null) {
            appState.openProduct(productId)
        } else {
            appState.prepareProductDetail()
        }
    }

    if (product == null) {
        LazyColumn(
            modifier = modifier
                .fillMaxSize()
                .padding(16.dp)
                .testTag(SmokeTag.ProductsScreen),
            verticalArrangement = Arrangement.spacedBy(12.dp),
        ) {
            item {
                RouteHeader(
                    title = "Edit product",
                    subtitle = "Loading catalogue details before editing.",
                    backLabel = "Back to product",
                    onBack = onCancel,
                )
            }
            item {
                StatusCard(
                    title = if (appState.productActionInFlight == ProductAction.LoadDetail) "Loading product" else "Product unavailable",
                    message = "Quartermaster is loading this product before it can be edited.",
                )
            }
        }
    } else {
        ProductFormScreen(
            appState = appState,
            product = product,
            modifier = modifier,
            onDone = onDone,
            onCancel = onCancel,
        )
    }
}

@Composable
internal fun ProductFormScreen(
    appState: QuartermasterAppState,
    product: ProductDto?,
    modifier: Modifier = Modifier,
    onDone: (ProductDto) -> Unit = {},
    onCancel: () -> Unit = {},
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
        item {
            RouteHeader(
                title = title,
                subtitle = "Manual products are scoped to the current household catalogue.",
                backLabel = "Cancel",
                onBack = onCancel,
            )
        }
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
                    modifier = Modifier
                        .weight(1f)
                        .testTag(SmokeTag.ProductSubmitButton),
                    onClick = {
                        scope.launch {
                            val saved = if (product == null) {
                                appState.createProduct(fields)
                            } else {
                                appState.updateSelectedProduct(fields)
                            }
                            saved?.let(onDone)
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
                TextButton(onClick = onCancel) {
                    Text("Cancel")
                }
            }
        }
        item {
            Spacer(Modifier.height(96.dp))
        }
    }
}

@Composable
internal fun ProductDeleteScreen(
    appState: QuartermasterAppState,
    productId: String,
    modifier: Modifier = Modifier,
    onDone: () -> Unit = {},
    onCancel: () -> Unit = {},
) {
    val scope = rememberCoroutineScope()
    val product = appState.selectedCatalogueProduct

    LaunchedEffect(productId) {
        if (product?.id?.toString() != productId) {
            appState.openProduct(productId)
        } else {
            appState.prepareProductDetail()
        }
    }

    LazyColumn(
        modifier = modifier
            .fillMaxSize()
            .padding(16.dp)
            .testTag(SmokeTag.ProductsScreen),
        verticalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        item {
            RouteHeader(
                title = "Delete product",
                subtitle = "Deleted manual products stay recoverable from the Deleted filter.",
                backLabel = "Back to product",
                onBack = onCancel,
            )
        }
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
                    onClick = {
                        scope.launch {
                            if (appState.deleteSelectedProduct()) {
                                onDone()
                            }
                        }
                    },
                    enabled = product != null && appState.productActionInFlight == null,
                ) {
                    Text(if (appState.productActionInFlight == ProductAction.Delete) "Deleting..." else "Delete product")
                }
                TextButton(onClick = onCancel) {
                    Text("Cancel")
                }
            }
        }
    }
}
