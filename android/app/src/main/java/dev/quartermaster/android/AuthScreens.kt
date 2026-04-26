package dev.quartermaster.android

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
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.unit.dp
import kotlinx.coroutines.launch

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
