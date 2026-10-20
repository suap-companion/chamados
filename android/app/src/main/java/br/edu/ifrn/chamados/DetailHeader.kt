package br.edu.ifrn.chamados

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Edit
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import uniffi.chamados_mobile.DraftKind
import uniffi.chamados_mobile.TicketField
import uniffi.chamados_mobile.TicketView

/**
 * O cabeçalho do chamado no padrão do DS gov.br: o título local é o H1 (sem título, o tipo do chamado);
 * com título, o tipo vem abaixo, em itálico; ao lado do H1, o botão de editar o título; depois a situação.
 */
@Composable
fun DetailHeader(model: MainViewModel, view: TicketView) {
    val kind = view.heading ?: view.title
    val title = model.titles[view.id]
    var editing by remember(view.id) { mutableStateOf(false) }
    Column(verticalArrangement = Arrangement.spacedBy(6.dp)) {
        Row(verticalAlignment = Alignment.Top, horizontalArrangement = Arrangement.spacedBy(4.dp)) {
            Text(
                title ?: kind,
                style = MaterialTheme.typography.headlineSmall,
                fontWeight = FontWeight.Bold,
                modifier = Modifier.weight(1f).semantics { heading() },
            )
            IconButton(onClick = { editing = true }) {
                Icon(Icons.Filled.Edit, contentDescription = "Editar o título", tint = MaterialTheme.colorScheme.primary)
            }
        }
        if (title != null) {
            Text(kind, style = MaterialTheme.typography.bodyMedium, fontStyle = FontStyle.Italic)
        }
        if (view.statuses.isNotEmpty()) {
            Row(horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                view.statuses.forEach { StatusBadge(it) }
            }
        }
        model.detailError?.let { Text(it, color = MaterialTheme.colorScheme.error) }
    }
    if (editing) TitleDialog(model, view) { editing = false }
}

/** O diálogo de editar o título local: salvar, remover (texto vazio) ou desistir. */
@Composable
private fun TitleDialog(model: MainViewModel, view: TicketView, onClose: () -> Unit) {
    val id = view.id
    val current = model.titles[id].orEmpty()
    var text by remember(id) { mutableStateOf(current) }
    val removing = text.isBlank() && current.isNotEmpty()
    AlertDialog(
        onDismissRequest = onClose,
        title = { Text("Título do chamado") },
        text = {
            Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                OutlinedTextField(
                    value = text, onValueChange = { text = it }, singleLine = true,
                    label = { Text("Título (só seu)") }, modifier = Modifier.fillMaxWidth(),
                )
                AssistButton(DraftKind.TITLE, view, current.ifEmpty { null }, text) { text = it }
                Text("O SUAP não tem título: ele fica só no seu aparelho e na sincronização.", style = MaterialTheme.typography.bodySmall)
            }
        },
        confirmButton = {
            TextButton(
                enabled = text.trim() != current,
                onClick = { model.saveTitle(id, text); onClose() },
            ) { Text(if (removing) "Remover título" else "Salvar") }
        },
        dismissButton = { TextButton(onClick = onClose) { Text("Desistir") } },
    )
}

/** A situação como etiqueta do DS (`br-tag`): cor pelo sentido da situação, com contraste AA. */
@Composable
fun StatusBadge(status: String) {
    val (background, content) = statusColors(status)
    Text(
        status,
        style = MaterialTheme.typography.labelLarge,
        color = content,
        modifier = Modifier.clip(RoundedCornerShape(100.dp)).background(background).padding(horizontal = 12.dp, vertical = 4.dp),
    )
}

private fun statusColors(status: String): Pair<Color, Color> {
    val s = status.lowercase()
    return when {
        "resolvid" in s || "fechad" in s -> GovBr.GreenCoolVivid5 to GovBr.GreenCoolVivid70
        "cancelad" in s -> GovBr.Gray10 to GovBr.Gray80
        "suspens" in s -> GovBr.RedVivid10 to GovBr.RedVivid70
        "atendimento" in s || "reaberto" in s -> GovBr.BlueWarmVivid10 to GovBr.BlueWarmVivid80
        else -> GovBr.BlueWarmVivid5 to GovBr.BlueWarmVivid70
    }
}

/** Um dado do chamado como item do DS: o rótulo pequeno e o valor, com divisor fino. */
@Composable
fun FieldItem(field: TicketField) {
    Column(Modifier.padding(top = 8.dp), verticalArrangement = Arrangement.spacedBy(2.dp)) {
        Text(field.label, style = MaterialTheme.typography.labelMedium, color = MaterialTheme.colorScheme.onSurfaceVariant)
        Text(field.value, style = MaterialTheme.typography.bodyLarge)
        HorizontalDivider(Modifier.padding(top = 8.dp), color = MaterialTheme.colorScheme.outlineVariant)
    }
}

/**
 * O interessado como no cabeçalho da conta: a foto (a mesma que a lista traz, quando o chamado veio dela),
 * o nome usual e, se o SUAP trouxer o campo, o campus. Sem foto, o ícone de pessoa.
 */
@Composable
fun InterestedItem(name: String, campus: String?, photo: String?) {
    Column(Modifier.padding(top = 8.dp), verticalArrangement = Arrangement.spacedBy(6.dp)) {
        Text("Interessado", style = MaterialTheme.typography.labelMedium, color = MaterialTheme.colorScheme.onSurfaceVariant)
        Row(horizontalArrangement = Arrangement.spacedBy(12.dp), verticalAlignment = Alignment.CenterVertically) {
            PersonAvatar(photo, size = 48.dp)
            Column {
                Text(name, style = MaterialTheme.typography.bodyLarge, fontWeight = FontWeight.Medium)
                if (campus != null) Text(campus, style = MaterialTheme.typography.bodyMedium, color = MaterialTheme.colorScheme.onSurfaceVariant)
            }
        }
        HorizontalDivider(Modifier.padding(top = 4.dp), color = MaterialTheme.colorScheme.outlineVariant)
    }
}
