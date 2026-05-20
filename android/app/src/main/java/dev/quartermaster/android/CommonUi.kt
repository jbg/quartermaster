package dev.quartermaster.android

import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.material3.Button
import androidx.compose.material3.Card
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp

internal enum class StatusTone {
    Available,
    Soon,
    Expired,
    Info,
    Neutral,
    Low,
    Frozen,
}

private data class StatusToneColors(
    val text: Color,
    val background: Color,
    val border: Color,
)

@Composable
private fun statusToneColors(tone: StatusTone): StatusToneColors = if (isSystemInDarkTheme()) {
    when (tone) {
        StatusTone.Available -> StatusToneColors(Color(0xFFA8D9B8), Color(0xFF1B3024), Color(0xFF355A42))
        StatusTone.Soon -> StatusToneColors(Color(0xFFE8C28F), Color(0xFF332619), Color(0xFF6D4D25))
        StatusTone.Expired -> StatusToneColors(Color(0xFFF0B7BE), Color(0xFF3A2023), Color(0xFF6F3C45))
        StatusTone.Info -> StatusToneColors(Color(0xFFA8D3E2), Color(0xFF1B303D), Color(0xFF355A68))
        StatusTone.Neutral -> StatusToneColors(Color(0xFFC7D0C7), Color(0xFF1F2D25), Color(0xFF2D3A32))
        StatusTone.Low -> StatusToneColors(Color(0xFFE2C98D), Color(0xFF342817), Color(0xFF66542A))
        StatusTone.Frozen -> StatusToneColors(Color(0xFFA8D3E2), Color(0xFF18313A), Color(0xFF315C68))
    }
} else {
    when (tone) {
        StatusTone.Available -> StatusToneColors(
            text = QuartermasterColors.Leaf,
            background = QuartermasterColors.SuccessBg,
            border = QuartermasterColors.SuccessBorder,
        )
        StatusTone.Soon -> StatusToneColors(
            text = Color(0xFF9A4F12),
            background = QuartermasterColors.WarningBg,
            border = QuartermasterColors.WarningBorder,
        )
        StatusTone.Expired -> StatusToneColors(
            text = QuartermasterColors.Beet,
            background = QuartermasterColors.DangerBg,
            border = QuartermasterColors.DangerBorder,
        )
        StatusTone.Info -> StatusToneColors(
            text = Color(0xFF245B73),
            background = QuartermasterColors.InfoBg,
            border = QuartermasterColors.InfoBorder,
        )
        StatusTone.Neutral -> StatusToneColors(
            text = QuartermasterColors.NeutralText,
            background = QuartermasterColors.NeutralBg,
            border = QuartermasterColors.NeutralBorder,
        )
        StatusTone.Low -> StatusToneColors(
            text = QuartermasterColors.LowText,
            background = QuartermasterColors.LowBg,
            border = QuartermasterColors.LowBorder,
        )
        StatusTone.Frozen -> StatusToneColors(
            text = QuartermasterColors.FrozenText,
            background = QuartermasterColors.FrozenBg,
            border = QuartermasterColors.FrozenBorder,
        )
    }
}

@Composable
internal fun CenteredLoading(modifier: Modifier = Modifier) {
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
internal fun MessageScreen(
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
internal fun SectionHeader(
    title: String,
    body: String? = null,
    modifier: Modifier = Modifier,
) {
    Column(
        modifier = modifier,
        verticalArrangement = Arrangement.spacedBy(4.dp),
    ) {
        Text(title, style = MaterialTheme.typography.titleMedium)
        body?.let {
            Text(
                it,
                style = MaterialTheme.typography.bodyMedium,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        }
    }
}

@Composable
internal fun RouteHeader(
    title: String,
    subtitle: String? = null,
    backLabel: String? = null,
    onBack: (() -> Unit)? = null,
    action: (@Composable () -> Unit)? = null,
) {
    Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
        if (backLabel != null && onBack != null) {
            TextButton(onClick = onBack) {
                Text(backLabel)
            }
        }
        Row(
            modifier = Modifier.fillMaxWidth(),
            horizontalArrangement = Arrangement.SpaceBetween,
        ) {
            Column(modifier = Modifier.weight(1f)) {
                Text(title, style = MaterialTheme.typography.headlineSmall)
                subtitle?.let {
                    Spacer(Modifier.height(4.dp))
                    Text(it, style = MaterialTheme.typography.bodyMedium)
                }
            }
            action?.let {
                Spacer(Modifier.width(12.dp))
                it()
            }
        }
    }
}

@Composable
internal fun StatusCard(
    title: String,
    message: String,
    modifier: Modifier = Modifier,
    actionLabel: String? = null,
    onAction: (() -> Unit)? = null,
) {
    Card(
        colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.surface),
        border = BorderStroke(1.dp, MaterialTheme.colorScheme.outlineVariant),
    ) {
        Column(
            modifier = modifier
                .fillMaxWidth()
                .padding(16.dp),
            verticalArrangement = Arrangement.spacedBy(6.dp),
        ) {
            Text(title, style = MaterialTheme.typography.titleMedium)
            Text(message, color = MaterialTheme.colorScheme.onSurfaceVariant)
            if (actionLabel != null && onAction != null) {
                TextButton(onClick = onAction) {
                    Text(actionLabel)
                }
            }
        }
    }
}

@Composable
internal fun ErrorCard(
    title: String,
    message: String,
    actionLabel: String? = null,
    onAction: (() -> Unit)? = null,
) {
    val colors = statusToneColors(StatusTone.Expired)
    Card(
        colors = CardDefaults.cardColors(containerColor = colors.background),
        border = BorderStroke(1.dp, colors.border),
    ) {
        Column(
            modifier = Modifier
                .fillMaxWidth()
                .padding(16.dp),
            verticalArrangement = Arrangement.spacedBy(6.dp),
        ) {
            Text(title, style = MaterialTheme.typography.titleMedium)
            Text(message, color = colors.text)
            if (actionLabel != null && onAction != null) {
                TextButton(onClick = onAction) {
                    Text(actionLabel)
                }
            }
        }
    }
}

@Composable
internal fun InlineStatusCard(
    title: String,
    message: String,
    modifier: Modifier = Modifier,
) {
    Row(
        modifier = modifier
            .fillMaxWidth()
            .padding(12.dp),
        horizontalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        CircularProgressIndicator()
        Column(verticalArrangement = Arrangement.spacedBy(3.dp)) {
            Text(title, style = MaterialTheme.typography.titleSmall)
            Text(
                message,
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        }
    }
}

@Composable
internal fun MetadataRow(label: String, value: String) {
    Column(verticalArrangement = Arrangement.spacedBy(2.dp)) {
        Text(
            label,
            style = MaterialTheme.typography.labelMedium,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
        Text(value, style = MaterialTheme.typography.bodyMedium)
    }
}

@Composable
internal fun StatusBadge(
    label: String,
    tone: StatusTone,
    modifier: Modifier = Modifier,
) {
    val colors = statusToneColors(tone)
    Box(
        modifier = modifier
            .background(colors.background, MaterialTheme.shapes.small)
            .border(1.dp, colors.border, MaterialTheme.shapes.small)
            .padding(horizontal = 8.dp, vertical = 4.dp),
    ) {
        Text(
            label,
            style = MaterialTheme.typography.labelSmall,
            color = colors.text,
            fontWeight = FontWeight.SemiBold,
        )
    }
}

@Composable
internal fun PrimarySecondaryActions(
    primaryLabel: String,
    onPrimary: () -> Unit,
    primaryEnabled: Boolean,
    secondaryLabel: String? = null,
    onSecondary: (() -> Unit)? = null,
    secondaryEnabled: Boolean = true,
    primaryModifier: Modifier = Modifier,
) {
    Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
        Button(
            modifier = primaryModifier,
            onClick = onPrimary,
            enabled = primaryEnabled,
        ) {
            Text(primaryLabel)
        }
        if (secondaryLabel != null && onSecondary != null) {
            TextButton(
                onClick = onSecondary,
                enabled = secondaryEnabled,
            ) {
                Text(secondaryLabel)
            }
        }
    }
}

@Composable
internal fun SelectionCard(
    title: String,
    options: List<Pair<String, String>>,
    selected: String?,
    emptyText: String,
    onSelect: (String) -> Unit,
) {
    Card {
        Column(
            modifier = Modifier
                .fillMaxWidth()
                .padding(16.dp),
            verticalArrangement = Arrangement.spacedBy(8.dp),
        ) {
            Text(title, style = MaterialTheme.typography.titleMedium)
            if (options.isEmpty()) {
                Text(emptyText)
            } else {
                options.forEach { (value, label) ->
                    val isSelected = value == selected
                    Row(
                        modifier = Modifier.fillMaxWidth(),
                        horizontalArrangement = Arrangement.SpaceBetween,
                    ) {
                        Text(label)
                        if (isSelected) {
                            Text("Selected", style = MaterialTheme.typography.labelMedium)
                        } else {
                            TextButton(onClick = { onSelect(value) }) {
                                Text("Select")
                            }
                        }
                    }
                }
            }
        }
    }
}
