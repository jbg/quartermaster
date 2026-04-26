package dev.quartermaster.android

import androidx.compose.material.icons.outlined.Settings
import androidx.compose.runtime.getValue
import androidx.compose.runtime.setValue

internal object AppRoute {
    const val Inventory = "inventory"
    const val Products = "products"
    const val Reminders = "reminders"
    const val Scan = "scan"
    const val Settings = "settings"
    const val BatchDetail = "inventory/batch/{batchId}"
    const val StockEdit = "inventory/batch/{batchId}/edit"
    const val ProductNew = "products/new"
    const val ProductDetail = "products/{productId}"
    const val ProductEdit = "products/{productId}/edit"
    const val ProductDelete = "products/{productId}/delete"
    const val LocationNew = "settings/locations/new"
    const val LocationEdit = "settings/locations/{locationId}/edit"
    const val LocationDelete = "settings/locations/{locationId}/delete"

    fun batchDetail(batchId: String) = "inventory/batch/$batchId"
    fun stockEdit(batchId: String) = "inventory/batch/$batchId/edit"
    fun productDetail(productId: String) = "products/$productId"
    fun productEdit(productId: String) = "products/$productId/edit"
    fun productDelete(productId: String) = "products/$productId/delete"
    fun locationEdit(locationId: String) = "settings/locations/$locationId/edit"
    fun locationDelete(locationId: String) = "settings/locations/$locationId/delete"
}

internal fun routeTab(route: String?): MainTab? = when (route) {
    AppRoute.Products,
    AppRoute.ProductNew,
    AppRoute.ProductDetail,
    AppRoute.ProductEdit,
    AppRoute.ProductDelete,
    -> MainTab.Products
    AppRoute.Reminders -> MainTab.Reminders
    AppRoute.Scan -> MainTab.Scan
    AppRoute.Settings,
    AppRoute.LocationNew,
    AppRoute.LocationEdit,
    AppRoute.LocationDelete,
    -> MainTab.Settings
    AppRoute.Inventory,
    AppRoute.BatchDetail,
    AppRoute.StockEdit,
    -> MainTab.Inventory
    null -> MainTab.Inventory
    else -> null
}

internal fun MainTab.route(): String = when (this) {
    MainTab.Inventory -> AppRoute.Inventory
    MainTab.Products -> AppRoute.Products
    MainTab.Reminders -> AppRoute.Reminders
    MainTab.Scan -> AppRoute.Scan
    MainTab.Settings -> AppRoute.Settings
}
