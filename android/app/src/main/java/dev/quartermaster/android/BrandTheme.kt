package dev.quartermaster.android

import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.material3.ColorScheme
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Shapes
import androidx.compose.material3.Typography
import androidx.compose.material3.darkColorScheme
import androidx.compose.material3.lightColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp

internal object QuartermasterColors {
    val Ink = Color(0xFF18201C)
    val Green900 = Color(0xFF173326)
    val Green800 = Color(0xFF234A35)
    val Green600 = Color(0xFF3F7658)
    val Sage100 = Color(0xFFE8EEE8)
    val Paper = Color(0xFFF7F8F4)
    val White = Color(0xFFFFFFFF)
    val Slate700 = Color(0xFF334039)
    val Slate500 = Color(0xFF66716B)
    val Steel = Color(0xFF7C8780)
    val Line = Color(0xFFD9DED6)
    val LineStrong = Color(0xFFC7D0C7)
    val Label = Color(0xFFF0EEE7)
    val Brass = Color(0xFFA66F2B)
    val Blueprint = Color(0xFF2F5F7A)
    val Beet = Color(0xFF8F2E3E)
    val BeetStrong = Color(0xFF9B2F2F)
    val Carrot = Color(0xFFC56B22)
    val Leaf = Color(0xFF2F7A4F)
    val SuccessBg = Color(0xFFE4F2EA)
    val SuccessBorder = Color(0xFFB9DEC8)
    val WarningBg = Color(0xFFFFF1DF)
    val WarningBorder = Color(0xFFE9C289)
    val DangerBg = Color(0xFFF8E2E5)
    val DangerBorder = Color(0xFFE3A8B1)
    val InfoBg = Color(0xFFE4F0F4)
    val InfoBorder = Color(0xFFB5D3DD)
    val NeutralBg = Color(0xFFEEF1EC)
    val NeutralText = Color(0xFF58645F)
    val NeutralBorder = Color(0xFFD5DCD4)
    val LowText = Color(0xFF7A4B12)
    val LowBg = Color(0xFFF6E8CC)
    val LowBorder = Color(0xFFE2C98D)
    val FrozenText = Color(0xFF1D5C73)
    val FrozenBg = Color(0xFFDDEFF4)
    val FrozenBorder = Color(0xFFACD2DE)

    val DarkApp = Color(0xFF101713)
    val DarkPanel = Color(0xFF17211B)
    val DarkSubtle = Color(0xFF1F2D25)
    val DarkPrimaryText = Color(0xFFF2F5EF)
    val DarkSecondaryText = Color(0xFFC7D0C7)
    val DarkMutedText = Color(0xFF9EA9A2)
    val DarkLine = Color(0xFF2D3A32)
    val DarkPrimary = Color(0xFF82B99A)
    val DarkPrimaryForeground = Color(0xFF0D1711)
    val DarkLink = Color(0xFF8FC7DE)
}

private val QuartermasterColorScheme: ColorScheme = lightColorScheme(
    primary = QuartermasterColors.Green800,
    onPrimary = QuartermasterColors.White,
    primaryContainer = QuartermasterColors.Sage100,
    onPrimaryContainer = QuartermasterColors.Green900,
    secondary = QuartermasterColors.Blueprint,
    onSecondary = QuartermasterColors.White,
    secondaryContainer = QuartermasterColors.InfoBg,
    onSecondaryContainer = QuartermasterColors.Slate700,
    tertiary = QuartermasterColors.Brass,
    onTertiary = QuartermasterColors.White,
    tertiaryContainer = QuartermasterColors.Label,
    onTertiaryContainer = QuartermasterColors.Slate700,
    error = QuartermasterColors.BeetStrong,
    onError = QuartermasterColors.White,
    errorContainer = QuartermasterColors.DangerBg,
    onErrorContainer = QuartermasterColors.BeetStrong,
    background = QuartermasterColors.Paper,
    onBackground = QuartermasterColors.Ink,
    surface = QuartermasterColors.White,
    onSurface = QuartermasterColors.Ink,
    surfaceVariant = QuartermasterColors.NeutralBg,
    onSurfaceVariant = QuartermasterColors.Slate700,
    outline = QuartermasterColors.LineStrong,
    outlineVariant = QuartermasterColors.Line,
    inverseSurface = QuartermasterColors.Green900,
    inverseOnSurface = QuartermasterColors.Paper,
    inversePrimary = QuartermasterColors.Sage100,
)

private val QuartermasterDarkColorScheme: ColorScheme = darkColorScheme(
    primary = QuartermasterColors.DarkPrimary,
    onPrimary = QuartermasterColors.DarkPrimaryForeground,
    primaryContainer = QuartermasterColors.DarkSubtle,
    onPrimaryContainer = QuartermasterColors.DarkPrimaryText,
    secondary = QuartermasterColors.DarkLink,
    onSecondary = Color(0xFF0D2633),
    secondaryContainer = Color(0xFF1B303D),
    onSecondaryContainer = Color(0xFFD7EAF3),
    tertiary = Color(0xFFD8A25F),
    onTertiary = Color(0xFF2D1A06),
    tertiaryContainer = Color(0xFF3A2A18),
    onTertiaryContainer = Color(0xFFFFDDB0),
    error = Color(0xFFFFB1B8),
    onError = Color(0xFF4E070E),
    errorContainer = Color(0xFF3A2023),
    onErrorContainer = Color(0xFFFFDADF),
    background = QuartermasterColors.DarkApp,
    onBackground = QuartermasterColors.DarkPrimaryText,
    surface = QuartermasterColors.DarkPanel,
    onSurface = QuartermasterColors.DarkPrimaryText,
    surfaceVariant = QuartermasterColors.DarkSubtle,
    onSurfaceVariant = QuartermasterColors.DarkSecondaryText,
    outline = QuartermasterColors.DarkLine,
    outlineVariant = Color(0xFF233028),
    inverseSurface = Color(0xFFEDF3EE),
    inverseOnSurface = Color(0xFF18201C),
    inversePrimary = Color(0xFF234A35),
)

private val QuartermasterShapes = Shapes(
    extraSmall = androidx.compose.foundation.shape.RoundedCornerShape(4.dp),
    small = androidx.compose.foundation.shape.RoundedCornerShape(6.dp),
    medium = androidx.compose.foundation.shape.RoundedCornerShape(8.dp),
    large = androidx.compose.foundation.shape.RoundedCornerShape(12.dp),
    extraLarge = androidx.compose.foundation.shape.RoundedCornerShape(12.dp),
)

private val QuartermasterTypography = Typography().let { base ->
    base.copy(
        headlineSmall = base.headlineSmall.copy(
            fontFamily = FontFamily.SansSerif,
            fontWeight = FontWeight.SemiBold,
        ),
        titleLarge = base.titleLarge.copy(fontWeight = FontWeight.SemiBold),
        titleMedium = base.titleMedium.copy(fontWeight = FontWeight.SemiBold),
        titleSmall = base.titleSmall.copy(fontWeight = FontWeight.SemiBold),
        labelMedium = base.labelMedium.copy(fontWeight = FontWeight.SemiBold),
    )
}

@Composable
internal fun QuartermasterTheme(content: @Composable () -> Unit) {
    val colorScheme = if (isSystemInDarkTheme()) QuartermasterDarkColorScheme else QuartermasterColorScheme

    MaterialTheme(
        colorScheme = colorScheme,
        shapes = QuartermasterShapes,
        typography = QuartermasterTypography,
        content = content,
    )
}
