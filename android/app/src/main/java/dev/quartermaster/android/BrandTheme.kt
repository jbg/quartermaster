package dev.quartermaster.android

import androidx.compose.material3.ColorScheme
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Typography
import androidx.compose.material3.lightColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight

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
    val WarningBg = Color(0xFFFFF1DF)
    val DangerBg = Color(0xFFF4E4E4)
    val InfoBg = Color(0xFFE4F0F4)
    val NeutralBg = Color(0xFFEEF1EC)
    val LowBg = Color(0xFFF6E8CC)
    val FrozenBg = Color(0xFFDDEFF4)
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

private val QuartermasterTypography = Typography().let { base ->
    base.copy(
        headlineSmall = base.headlineSmall.copy(
            fontFamily = FontFamily.SansSerif,
            fontWeight = FontWeight.SemiBold,
            color = QuartermasterColors.Green900,
        ),
        titleLarge = base.titleLarge.copy(fontWeight = FontWeight.SemiBold),
        titleMedium = base.titleMedium.copy(fontWeight = FontWeight.SemiBold),
        titleSmall = base.titleSmall.copy(fontWeight = FontWeight.SemiBold),
        labelMedium = base.labelMedium.copy(fontWeight = FontWeight.SemiBold),
    )
}

@Composable
internal fun QuartermasterTheme(content: @Composable () -> Unit) {
    MaterialTheme(
        colorScheme = QuartermasterColorScheme,
        typography = QuartermasterTypography,
        content = content,
    )
}
