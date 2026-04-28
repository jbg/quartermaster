package dev.quartermaster.android

import androidx.compose.foundation.layout.padding
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.outlined.Category
import androidx.compose.material.icons.outlined.Inventory2
import androidx.compose.material.icons.outlined.Notifications
import androidx.compose.material.icons.outlined.QrCodeScanner
import androidx.compose.material.icons.outlined.Tune
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.NavigationBar
import androidx.compose.material3.NavigationBarDefaults
import androidx.compose.material3.NavigationBarItem
import androidx.compose.material3.Scaffold
import androidx.compose.material3.SnackbarHost
import androidx.compose.material3.SnackbarHostState
import androidx.compose.material3.Text
import androidx.compose.material3.TopAppBar
import androidx.compose.material3.TopAppBarDefaults
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.ExperimentalComposeUiApi
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.testTagsAsResourceId
import androidx.navigation.NavHostController
import androidx.navigation.NavType
import androidx.navigation.compose.NavHost
import androidx.navigation.compose.composable
import androidx.navigation.compose.currentBackStackEntryAsState
import androidx.navigation.compose.rememberNavController
import androidx.navigation.navArgument

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
        routeTab(currentRoute)?.let { appState.selectedTab = it }
    }
    LaunchedEffect(appState.selectedTab, currentRoute, appState.currentHouseholdId) {
        val route = appState.selectedTab.route()
        if (
            appState.currentHouseholdId != null &&
            currentRoute != null &&
            routeTab(currentRoute) != appState.selectedTab
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

    QuartermasterTheme {
        Scaffold(
            modifier = Modifier.semantics { testTagsAsResourceId = true },
            containerColor = MaterialTheme.colorScheme.background,
            snackbarHost = { SnackbarHost(hostState = snackbarHostState) },
            topBar = {
                TopAppBar(
                    title = { Text("Quartermaster") },
                    colors = TopAppBarDefaults.topAppBarColors(
                        containerColor = MaterialTheme.colorScheme.background,
                        titleContentColor = MaterialTheme.colorScheme.onBackground,
                    ),
                )
            },
            bottomBar = {
                if (appState.phase is AppPhase.Authenticated && appState.currentHouseholdId != null) {
                    NavigationBar(
                        containerColor = MaterialTheme.colorScheme.surface,
                        tonalElevation = NavigationBarDefaults.Elevation,
                    ) {
                        listOf(
                            MainTab.Inventory to Pair("Inventory", Icons.Outlined.Inventory2),
                            MainTab.Products to Pair("Products", Icons.Outlined.Category),
                            MainTab.Reminders to Pair("Reminders", Icons.Outlined.Notifications),
                            MainTab.Scan to Pair("Scan", Icons.Outlined.QrCodeScanner),
                            MainTab.Settings to Pair("Settings", Icons.Outlined.Tune),
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
                onOpenBatch = { batchId -> navController.navigate(AppRoute.batchDetail(batchId)) },
            )
        }
        composable(
            route = AppRoute.BatchDetail,
            arguments = listOf(navArgument("batchId") { type = NavType.StringType }),
        ) { entry ->
            val batchId = entry.arguments?.getString("batchId").orEmpty()
            BatchDetailScreen(
                appState = appState,
                batchId = batchId,
                onBack = {
                    appState.clearSelectedBatch()
                    navController.popBackStack(AppRoute.Inventory, inclusive = false)
                },
                onEditBatch = { navController.navigate(AppRoute.stockEdit(batchId)) },
            )
        }
        composable(AppRoute.Products) {
            ProductListScreen(
                appState = appState,
                onCreateProduct = {
                    appState.prepareProductCreate()
                    navController.navigate(AppRoute.ProductNew)
                },
                onOpenProduct = { productId -> navController.navigate(AppRoute.productDetail(productId)) },
            )
        }
        composable(AppRoute.ProductNew) {
            ProductFormScreen(
                appState = appState,
                product = null,
                onDone = { product ->
                    if (appState.selectedTab == MainTab.Scan) {
                        navController.navigate(AppRoute.Scan) {
                            popUpTo(AppRoute.Products) { inclusive = true }
                            launchSingleTop = true
                        }
                    } else {
                        navController.navigate(AppRoute.productDetail(product.id.toString())) {
                            popUpTo(AppRoute.Products)
                        }
                    }
                },
                onCancel = {
                    if (appState.cancelProductFormForScan()) {
                        navController.navigate(AppRoute.Scan) {
                            popUpTo(AppRoute.Products) { inclusive = true }
                            launchSingleTop = true
                        }
                    } else {
                        navController.popBackStack(AppRoute.Products, inclusive = false)
                    }
                },
            )
        }
        composable(
            route = AppRoute.ProductDetail,
            arguments = listOf(navArgument("productId") { type = NavType.StringType }),
        ) { entry ->
            val productId = entry.arguments?.getString("productId").orEmpty()
            ProductDetailScreen(
                appState = appState,
                productId = productId,
                onBack = {
                    appState.prepareProductList()
                    navController.popBackStack(AppRoute.Products, inclusive = false)
                },
                onEdit = { navController.navigate(AppRoute.productEdit(productId)) },
                onDelete = { navController.navigate(AppRoute.productDelete(productId)) },
            )
        }
        composable(
            route = AppRoute.ProductEdit,
            arguments = listOf(navArgument("productId") { type = NavType.StringType }),
        ) { entry ->
            val productId = entry.arguments?.getString("productId").orEmpty()
            ProductEditScreen(
                appState = appState,
                productId = productId,
                onDone = { product ->
                    navController.navigate(AppRoute.productDetail(product.id.toString())) {
                        popUpTo(AppRoute.Products)
                    }
                },
                onCancel = { navController.popBackStack() },
            )
        }
        composable(
            route = AppRoute.ProductDelete,
            arguments = listOf(navArgument("productId") { type = NavType.StringType }),
        ) { entry ->
            val productId = entry.arguments?.getString("productId").orEmpty()
            ProductDeleteScreen(
                appState = appState,
                productId = productId,
                onDone = { navController.popBackStack(AppRoute.Products, inclusive = false) },
                onCancel = { navController.popBackStack() },
            )
        }
        composable(AppRoute.Reminders) { ReminderScreen(appState) }
        composable(AppRoute.Scan) {
            ScanScreen(
                appState = appState,
                onCreateProduct = {
                    appState.prepareProductCreateForScan()
                    navController.navigate(AppRoute.ProductNew)
                },
            )
        }
        composable(AppRoute.Settings) {
            SettingsScreen(
                appState = appState,
                onCreateLocation = { navController.navigate(AppRoute.LocationNew) },
                onEditLocation = { locationId -> navController.navigate(AppRoute.locationEdit(locationId)) },
                onDeleteLocation = { locationId -> navController.navigate(AppRoute.locationDelete(locationId)) },
            )
        }
        composable(AppRoute.LocationNew) {
            LocationFormScreen(
                appState = appState,
                locationId = null,
                onDone = { navController.popBackStack(AppRoute.Settings, inclusive = false) },
                onCancel = { navController.popBackStack(AppRoute.Settings, inclusive = false) },
            )
        }
        composable(
            route = AppRoute.LocationEdit,
            arguments = listOf(navArgument("locationId") { type = NavType.StringType }),
        ) { entry ->
            LocationFormScreen(
                appState = appState,
                locationId = entry.arguments?.getString("locationId").orEmpty(),
                onDone = { navController.popBackStack(AppRoute.Settings, inclusive = false) },
                onCancel = { navController.popBackStack(AppRoute.Settings, inclusive = false) },
            )
        }
        composable(
            route = AppRoute.LocationDelete,
            arguments = listOf(navArgument("locationId") { type = NavType.StringType }),
        ) { entry ->
            LocationDeleteScreen(
                appState = appState,
                locationId = entry.arguments?.getString("locationId").orEmpty(),
                onDone = { navController.popBackStack(AppRoute.Settings, inclusive = false) },
                onCancel = { navController.popBackStack(AppRoute.Settings, inclusive = false) },
            )
        }
        composable(
            route = AppRoute.StockEdit,
            arguments = listOf(navArgument("batchId") { type = NavType.StringType }),
        ) { entry ->
            val batchId = entry.arguments?.getString("batchId").orEmpty()
            StockEditScreen(
                appState = appState,
                batchId = batchId,
                onDone = { navController.popBackStack() },
                onCancel = { navController.popBackStack() },
            )
        }
    }
}
