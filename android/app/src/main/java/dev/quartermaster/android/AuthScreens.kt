package dev.quartermaster.android

import android.Manifest
import android.content.pm.PackageManager
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
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
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.unit.dp
import androidx.core.content.ContextCompat
import dev.quartermaster.android.generated.models.OnboardingAvailability
import dev.quartermaster.android.generated.models.OnboardingServerState
import kotlinx.coroutines.launch
import java.net.URI
import java.util.TimeZone

@Composable
internal fun InviteHandoffCard(
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
internal fun OnboardingScreen(appState: QuartermasterAppState, modifier: Modifier = Modifier) {
    val scope = rememberCoroutineScope()
    val context = LocalContext.current
    var username by remember { mutableStateOf("") }
    var password by remember { mutableStateOf("") }
    var householdName by remember { mutableStateOf("") }
    var timezone by remember { mutableStateOf(TimeZone.getDefault().id) }
    var serverUrl by remember { mutableStateOf(appState.serverUrl) }
    var advancedExpanded by remember { mutableStateOf(false) }
    var signInMode by remember { mutableStateOf(true) }
    var localError by remember { mutableStateOf<String?>(null) }
    var showSetupScanner by remember { mutableStateOf(false) }

    val permissionLauncher = rememberLauncherForActivityResult(ActivityResultContracts.RequestPermission()) { granted ->
        if (granted) {
            showSetupScanner = true
        } else {
            localError = "Camera access is needed to scan setup codes. Enter the server URL in Advanced."
            advancedExpanded = true
        }
    }

    LaunchedEffect(appState.pendingInviteContext) {
        appState.pendingInviteContext?.let { context ->
            serverUrl = context.serverUrl ?: appState.serverUrl
            scope.launch { appState.refreshOnboardingStatus() }
        }
    }

    if (showSetupScanner) {
        SetupQrScannerScreen(
            onCode = { contents ->
                showSetupScanner = false
                handleSetupPayload(contents, appState, onError = { localError = it })
                serverUrl = appState.serverUrl
                scope.launch { appState.refreshOnboardingStatus() }
            },
            onCancel = { showSetupScanner = false },
            onError = {
                localError = it
                advancedExpanded = true
            },
            modifier = modifier,
        )
        return
    }

    LazyColumn(
        modifier = modifier
            .testTag(SmokeTag.OnboardingScreen)
            .fillMaxSize()
            .padding(16.dp),
        verticalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        item {
            Text("Kitchen inventory, kept in order.", style = MaterialTheme.typography.headlineSmall)
        }
        appState.lastError?.let { message ->
            item { ErrorCard("Onboarding failed", message) }
        }
        localError?.let { message ->
            item { ErrorCard("Setup code failed", message) }
        }
        if (appState.onboardingStatus == null) {
            item {
                Button(
                    onClick = {
                        if (
                            ContextCompat.checkSelfPermission(context, Manifest.permission.CAMERA) ==
                            PackageManager.PERMISSION_GRANTED
                        ) {
                            showSetupScanner = true
                        } else {
                            permissionLauncher.launch(Manifest.permission.CAMERA)
                        }
                    },
                    modifier = Modifier.fillMaxWidth(),
                    enabled = !appState.authActionInFlight,
                ) {
                    Text("Scan setup code")
                }
            }
            item {
                TextButton(onClick = { advancedExpanded = !advancedExpanded }) {
                    Text("Advanced")
                }
            }
            if (advancedExpanded) {
                item {
                    OutlinedTextField(
                        value = serverUrl,
                        onValueChange = { serverUrl = it },
                        label = { Text("Server URL") },
                        modifier = Modifier
                            .fillMaxWidth()
                            .testTag(SmokeTag.ServerUrlField),
                    )
                }
                item {
                    Button(
                        onClick = {
                            appState.updateServerUrl(serverUrl)
                            scope.launch { appState.refreshOnboardingStatus() }
                        },
                        enabled = !appState.authActionInFlight && serverUrl.isNotBlank(),
                    ) {
                        Text(if (appState.authActionInFlight) "Connecting..." else "Connect")
                    }
                }
            }
        } else {
            item {
                Text("Connected to ${appState.serverUrl}", style = MaterialTheme.typography.bodyMedium)
                TextButton(
                    onClick = {
                        appState.clearPendingInviteContext()
                        appState.clearOnboardingStatus()
                    },
                ) { Text("Change server") }
            }
            appState.pendingInviteCode?.let { inviteCode ->
                item {
                    InviteHandoffCard(
                        inviteCode = inviteCode,
                        onDismiss = appState::clearPendingInviteContext,
                    )
                }
                item {
                    AccountFields(username, password, onUsername = { username = it }, onPassword = { password = it })
                }
                item {
                    Button(
                        modifier = Modifier.testTag(SmokeTag.SignInButton),
                        onClick = {
                            scope.launch {
                                appState.joinOnboardingInvite(
                                    username = username.trim(),
                                    password = password,
                                    inviteCode = inviteCode,
                                )
                            }
                        },
                        enabled = !appState.authActionInFlight && username.isNotBlank() && password.length >= 8,
                    ) {
                        Text(if (appState.authActionInFlight) "Joining..." else "Join household")
                    }
                }
            } ?: run {
                val status = appState.onboardingStatus
                if (status?.serverState == OnboardingServerState.NEEDS_INITIAL_SETUP) {
                    item {
                        AccountFields(username, password, onUsername = { username = it }, onPassword = { password = it })
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
                            modifier = Modifier.testTag(SmokeTag.SignInButton),
                            onClick = {
                                scope.launch {
                                    appState.createOnboardingHousehold(
                                        username = username.trim(),
                                        password = password,
                                        householdName = householdName.trim(),
                                        timezone = timezone.trim(),
                                    )
                                }
                            },
                            enabled = !appState.authActionInFlight &&
                                username.isNotBlank() &&
                                password.length >= 8 &&
                                householdName.isNotBlank() &&
                                timezone.isNotBlank(),
                        ) {
                            Text(if (appState.authActionInFlight) "Setting up..." else "Set up server")
                        }
                    }
                } else {
                    item {
                        Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                            Button(onClick = { signInMode = true }) { Text("Sign in") }
                            if (status?.householdSignup == OnboardingAvailability.ENABLED) {
                                TextButton(onClick = { signInMode = false }) { Text("Create household") }
                            }
                        }
                    }
                    item {
                        AccountFields(username, password, onUsername = { username = it }, onPassword = { password = it })
                    }
                    if (!signInMode && status?.householdSignup == OnboardingAvailability.ENABLED) {
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
                    }
                    item {
                        Button(
                            modifier = Modifier.testTag(SmokeTag.SignInButton),
                            onClick = {
                                scope.launch {
                                    if (signInMode) {
                                        appState.signIn(username = username.trim(), password = password)
                                    } else {
                                        appState.createOnboardingHousehold(
                                            username = username.trim(),
                                            password = password,
                                            householdName = householdName.trim(),
                                            timezone = timezone.trim(),
                                        )
                                    }
                                }
                            },
                            enabled = !appState.authActionInFlight &&
                                username.isNotBlank() &&
                                password.length >= 8 &&
                                (signInMode || (householdName.isNotBlank() && timezone.isNotBlank())),
                        ) {
                            Text(
                                when {
                                    appState.authActionInFlight -> "Working..."
                                    signInMode -> "Sign in"
                                    else -> "Create household"
                                },
                            )
                        }
                    }
                }
            }
        }
    }
}

@Composable
private fun AccountFields(
    username: String,
    password: String,
    onUsername: (String) -> Unit,
    onPassword: (String) -> Unit,
) {
    Column(verticalArrangement = Arrangement.spacedBy(12.dp)) {
        OutlinedTextField(
            value = username,
            onValueChange = onUsername,
            label = { Text("Username") },
            modifier = Modifier
                .fillMaxWidth()
                .testTag(SmokeTag.UsernameField),
        )
        OutlinedTextField(
            value = password,
            onValueChange = onPassword,
            label = { Text("Password") },
            modifier = Modifier
                .fillMaxWidth()
                .testTag(SmokeTag.PasswordField),
        )
    }
}

internal fun handleSetupPayload(
    payload: String,
    appState: QuartermasterAppState,
    onError: (String) -> Unit,
) {
    val context = QuartermasterAppState.parseInviteContext(payload)
    if (context != null) {
        appState.handleDeepLink(payload)
        return
    }
    val uri = runCatching { URI(payload) }.getOrNull()
    val raw = uri?.toString()?.trim().orEmpty()
    if (raw.startsWith("http://") || raw.startsWith("https://")) {
        appState.updateServerUrl(raw)
    } else {
        onError("That setup code is not a Quartermaster link.")
    }
}

@Composable
internal fun NoHouseholdScreen(appState: QuartermasterAppState, modifier: Modifier = Modifier) {
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
