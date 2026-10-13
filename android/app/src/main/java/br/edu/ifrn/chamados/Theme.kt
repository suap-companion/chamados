package br.edu.ifrn.chamados

import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Shapes
import androidx.compose.material3.Typography
import androidx.compose.material3.darkColorScheme
import androidx.compose.material3.lightColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp

/**
 * Tema do app a partir do Design System do gov.br (https://www.gov.br/ds/), versão estável 3.x
 * (`@govbr-ds/core`): os valores abaixo são os tokens da paleta (`--blue-warm-vivid-70` = #1351B4 etc.),
 * ligados aos papéis do Material 3. Conferidos por contraste (WCAG AA, 4,5:1 para texto) em docs/android.md.
 *
 * Não embute a fonte Rawline (a do DS): usa a do sistema até a licença de redistribuição ser confirmada.
 */
object GovBr {
    // Paleta (tokens do DS)
    val BlueWarmVivid90 = Color(0xFF071D41)
    val BlueWarmVivid80 = Color(0xFF0C326F)
    val BlueWarmVivid70 = Color(0xFF1351B4) // --interactive
    val BlueWarmVivid60 = Color(0xFF155BCB) // --info
    val BlueWarmVivid30 = Color(0xFF81AEFC)
    val BlueWarmVivid20 = Color(0xFFADCDFF)
    val BlueWarmVivid10 = Color(0xFFD4E5FF)
    val BlueWarmVivid5 = Color(0xFFEDF5FF)
    val BlueWarm20 = Color(0xFFC5D4EB) // --interactive-dark
    val Gray90 = Color(0xFF1B1B1B)
    val Gray80 = Color(0xFF333333) // --color
    val Gray70 = Color(0xFF555555)
    val Gray40 = Color(0xFF888888) // --border-color-alternative
    val Gray20 = Color(0xFFCCCCCC) // --border-color
    val Gray10 = Color(0xFFE6E6E6)
    val Gray5 = Color(0xFFF0F0F0) // --background-alternative
    val Gray3 = Color(0xFFF6F6F6)
    val Gray2 = Color(0xFFF8F8F8)
    val GreenCoolVivid70 = Color(0xFF154C21)
    val GreenCoolVivid60 = Color(0xFF216E1F)
    val GreenCoolVivid50 = Color(0xFF168821) // --success
    val GreenCoolVivid10 = Color(0xFFB7F5BD)
    val GreenCoolVivid5 = Color(0xFFE3F5E1)
    val RedVivid70 = Color(0xFF8B0A03)
    val RedVivid60 = Color(0xFFB50909)
    val RedVivid50 = Color(0xFFE52207) // --danger
    val RedVivid10 = Color(0xFFFDE0DB)
    val White = Color(0xFFFFFFFF)
    val Black = Color(0xFF000000)

    val Light = lightColorScheme(
        primary = BlueWarmVivid70,
        onPrimary = White,
        primaryContainer = BlueWarmVivid10,
        onPrimaryContainer = BlueWarmVivid90,
        secondary = BlueWarmVivid80,
        onSecondary = White,
        secondaryContainer = BlueWarmVivid10,
        onSecondaryContainer = BlueWarmVivid90,
        tertiary = GreenCoolVivid60,
        onTertiary = White,
        tertiaryContainer = GreenCoolVivid5,
        onTertiaryContainer = GreenCoolVivid70,
        error = RedVivid60,
        onError = White,
        errorContainer = RedVivid10,
        onErrorContainer = RedVivid70,
        background = White,
        onBackground = Gray80,
        surface = White,
        onSurface = Gray80,
        surfaceVariant = Gray5,
        onSurfaceVariant = Gray70,
        surfaceTint = BlueWarmVivid70,
        inverseSurface = Gray80,
        inverseOnSurface = White,
        inversePrimary = BlueWarmVivid30,
        outline = Gray40,
        outlineVariant = Gray20,
        scrim = Black,
        surfaceContainerLowest = White,
        surfaceContainerLow = Gray2,
        surfaceContainer = Gray3,
        surfaceContainerHigh = Gray5,
        surfaceContainerHighest = Gray10,
    )

    val Dark = darkColorScheme(
        primary = BlueWarm20, // --interactive-dark
        onPrimary = BlueWarmVivid90,
        primaryContainer = BlueWarmVivid70,
        onPrimaryContainer = White,
        secondary = BlueWarmVivid20,
        onSecondary = BlueWarmVivid90,
        secondaryContainer = BlueWarmVivid80,
        onSecondaryContainer = BlueWarmVivid10,
        tertiary = GreenCoolVivid10,
        onTertiary = GreenCoolVivid70,
        tertiaryContainer = GreenCoolVivid60,
        onTertiaryContainer = White,
        error = RedVivid10,
        onError = RedVivid70,
        errorContainer = RedVivid60,
        onErrorContainer = White,
        background = BlueWarmVivid90, // --background-dark
        onBackground = White, // --color-dark
        surface = BlueWarmVivid90,
        onSurface = White,
        surfaceVariant = Color(0xFF0C2A5C),
        onSurfaceVariant = BlueWarmVivid10,
        surfaceTint = BlueWarm20,
        inverseSurface = White,
        inverseOnSurface = BlueWarmVivid90,
        inversePrimary = BlueWarmVivid70,
        outline = BlueWarmVivid30,
        outlineVariant = BlueWarmVivid80,
        scrim = Black,
        surfaceContainerLowest = Color(0xFF051630),
        surfaceContainerLow = Color(0xFF0A2149),
        surfaceContainer = Color(0xFF0C2A5C),
        surfaceContainerHigh = Color(0xFF0C326F),
        surfaceContainerHighest = Color(0xFF123B80),
    )

    /** Cantos do DS: 4, 8 e 12 dp (`--surface-rounder-sm|md|lg`). */
    val Shapes = Shapes(
        extraSmall = RoundedCornerShape(4.dp),
        small = RoundedCornerShape(4.dp),
        medium = RoundedCornerShape(8.dp),
        large = RoundedCornerShape(12.dp),
        extraLarge = RoundedCornerShape(12.dp),
    )

    /** Entrelinha de 1,45 (`--font-line-height-medium`) no texto corrido; títulos em negrito médio. */
    private val base = Typography()
    val Type = base.copy(
        titleLarge = base.titleLarge.copy(fontWeight = androidx.compose.ui.text.font.FontWeight.Medium),
        titleMedium = base.titleMedium.copy(fontWeight = androidx.compose.ui.text.font.FontWeight.Medium),
        bodyLarge = body(base.bodyLarge, 16),
        bodyMedium = body(base.bodyMedium, 14),
        bodySmall = body(base.bodySmall, 12),
    )

    private fun body(style: TextStyle, size: Int) = style.copy(lineHeight = (size * 1.45).sp)
}

/** O tema do app: claro ou escuro, conforme o sistema. */
@Composable
fun ChamadosTheme(content: @Composable () -> Unit) {
    val colors = if (isSystemInDarkTheme()) GovBr.Dark else GovBr.Light
    MaterialTheme(colorScheme = colors, shapes = GovBr.Shapes, typography = GovBr.Type, content = content)
}
