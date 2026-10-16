package br.edu.ifrn.chamados

import android.graphics.BitmapFactory
import android.provider.Settings
import androidx.compose.animation.core.RepeatMode
import androidx.compose.animation.core.animateFloat
import androidx.compose.animation.core.infiniteRepeatable
import androidx.compose.animation.core.rememberInfiniteTransition
import androidx.compose.animation.core.tween
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Person
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.selected
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import uniffi.chamados_mobile.SnoozeItem
import uniffi.chamados_mobile.TicketSummary

/**
 * A listagem no padrão do Design System gov.br: Item (`br-item`) dentro de uma Lista (`br-list`), com
 * divisores finos, padding de 16 dp, alvo de toque de 48 dp, e o skeleton do padrão de Loading.
 */

/** Um chamado: avatar, número e situação, marca do adiamento, título local e assunto. Um único alvo de toque. */
@Composable
fun TicketItem(
    ticket: TicketSummary,
    title: String?,
    snooze: SnoozeItem?,
    photo: String? = null,
    selected: Boolean = false,
    onClick: () -> Unit,
) {
    val note = snooze?.let { if (it.due) "voltou do adiamento" else "volta ${formatMoment(it.until)}" }
    val description = listOfNotNull(
        "chamado ${ticket.id}", ticket.status, note, title, ticket.subject ?: "sem assunto",
    ).joinToString(", ")
    val background = if (selected) MaterialTheme.colorScheme.primaryContainer else MaterialTheme.colorScheme.surface
    Row(
        Modifier
            .fillMaxWidth()
            .heightIn(min = 48.dp)
            .background(background)
            .clickable(onClick = onClick)
            .semantics(mergeDescendants = true) {
                contentDescription = description
                this.selected = selected
            }
            .padding(16.dp),
        horizontalArrangement = Arrangement.spacedBy(16.dp),
        verticalAlignment = Alignment.Top,
    ) {
        Avatar(photo)
        Column(Modifier.weight(1f).clearAndSetSemantics { }, verticalArrangement = Arrangement.spacedBy(2.dp)) {
            Text("#${ticket.id}  ${ticket.status.orEmpty()}", style = MaterialTheme.typography.labelLarge)
            if (snooze != null && note != null) {
                val mark = if (snooze.due) "↩ " else "⏰ "
                Text(mark + note, style = MaterialTheme.typography.labelMedium, color = MaterialTheme.colorScheme.primary)
            }
            title?.let { Text(it, style = MaterialTheme.typography.titleMedium) }
            Text(ticket.subject ?: "(sem assunto)", style = MaterialTheme.typography.bodyLarge)
        }
    }
}

/** O avatar do solicitante: a foto da lista, ou o ícone de pessoa enquanto não há (ou não baixou). */
@Composable
private fun Avatar(photo: String?) {
    val bitmap = remember(photo) {
        photo?.let { path -> runCatching { BitmapFactory.decodeFile(path)?.asImageBitmap() }.getOrNull() }
    }
    Box(
        Modifier.size(40.dp).clip(CircleShape).background(MaterialTheme.colorScheme.surfaceVariant),
        contentAlignment = Alignment.Center,
    ) {
        if (bitmap != null) {
            Image(bitmap, contentDescription = null, contentScale = ContentScale.Crop, modifier = Modifier.size(40.dp))
        } else {
            Icon(Icons.Filled.Person, contentDescription = null, tint = MaterialTheme.colorScheme.onSurfaceVariant)
        }
    }
}

/** O divisor fino entre os itens da lista. */
@Composable
fun ListDivider() {
    HorizontalDivider(color = MaterialTheme.colorScheme.outlineVariant)
}

/** Se o sistema pede para reduzir animações (escala zero): o skeleton fica parado. */
@Composable
private fun reducedMotion(): Boolean {
    val resolver = LocalContext.current.contentResolver
    return Settings.Global.getFloat(resolver, Settings.Global.ANIMATOR_DURATION_SCALE, 1f) == 0f
}

/** O skeleton do Loading: itens com a forma dos chamados, em barras cinzentas que pulsam. */
@Composable
fun TicketListSkeleton(count: Int = 6) {
    val still = reducedMotion()
    val pulse by rememberInfiniteTransition(label = "skeleton").animateFloat(
        initialValue = 1f,
        targetValue = if (still) 1f else 0.4f,
        animationSpec = infiniteRepeatable(tween(900), RepeatMode.Reverse),
        label = "skeleton-alpha",
    )
    Column(Modifier.semantics { contentDescription = "Carregando chamados"; liveRegion = LiveRegionMode.Polite }) {
        repeat(count) { index ->
            Row(Modifier.fillMaxWidth().padding(16.dp).alpha(pulse), horizontalArrangement = Arrangement.spacedBy(16.dp)) {
                Box(Modifier.size(40.dp).clip(CircleShape).background(MaterialTheme.colorScheme.surfaceVariant))
                Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                    Bar(0.35f, 12)
                    Bar(0.9f, 16)
                    Bar(0.6f, 12)
                }
            }
            if (index < count - 1) ListDivider()
        }
    }
}

@Composable
private fun Bar(fraction: Float, height: Int) {
    Box(
        Modifier
            .fillMaxWidth(fraction)
            .heightIn(min = height.dp)
            .clip(RoundedCornerShape(4.dp))
            .background(MaterialTheme.colorScheme.surfaceVariant),
    )
}

/** Estado vazio ou de erro: uma mensagem curta e, no erro, a ação "Tentar de novo". */
@Composable
fun ListState(text: String, isError: Boolean, onRetry: (() -> Unit)? = null) {
    Column(
        Modifier.fillMaxWidth().padding(24.dp),
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        Text(
            text,
            style = MaterialTheme.typography.bodyLarge,
            color = if (isError) MaterialTheme.colorScheme.error else MaterialTheme.colorScheme.onSurface,
        )
        if (onRetry != null) TextButton(onClick = onRetry) { Text("Tentar de novo") }
    }
}
