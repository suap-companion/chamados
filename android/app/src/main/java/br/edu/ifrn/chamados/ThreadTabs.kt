package br.edu.ifrn.chamados

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyListScope
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Badge
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.ScrollableTabRow
import androidx.compose.material3.Tab
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.text.font.FontStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import uniffi.chamados_mobile.AttachmentItem
import uniffi.chamados_mobile.EntryKind
import uniffi.chamados_mobile.ThreadItem
import uniffi.chamados_mobile.TicketView

/** As abas da linha do tempo do chamado: tudo junto e cada tipo à parte. */
enum class ThreadTab(val label: String) {
    All("Tudo"),
    Comments("Comentários"),
    Notes("Notas internas"),
    Attachments("Anexos"),
}

/** As abas que o chamado tem: as notas internas só aparecem para quem é da equipe de atendimento. */
fun threadTabs(view: TicketView): List<ThreadTab> =
    ThreadTab.entries.filter { it != ThreadTab.Notes || view.canSeeNotes }

/** Tudo da linha do tempo: os comentários, os acontecimentos, os anexos e as notas, na mesma ordem das datas. */
fun allEntries(view: TicketView): List<ThreadItem> =
    (view.thread + view.notes).sortedByDescending { it.sortKey }

fun entriesOf(view: TicketView, tab: ThreadTab): List<ThreadItem> = when (tab) {
    ThreadTab.All -> allEntries(view)
    ThreadTab.Comments -> view.thread.filter { it.kind == EntryKind.COMMENT }
    ThreadTab.Notes -> view.notes
    ThreadTab.Attachments -> emptyList()
}

fun countOf(view: TicketView, tab: ThreadTab): Int = when (tab) {
    ThreadTab.All -> view.thread.size + view.notes.size
    ThreadTab.Attachments -> view.attachments.size
    else -> entriesOf(view, tab).size
}

/** O seletor de abas (com a quantidade de cada uma). */
@Composable
fun ThreadTabRow(view: TicketView, tab: ThreadTab, onTab: (ThreadTab) -> Unit) {
    val tabs = threadTabs(view)
    ScrollableTabRow(selectedTabIndex = tabs.indexOf(tab).coerceAtLeast(0), edgePadding = 0.dp) {
        tabs.forEach { item ->
            val amount = countOf(view, item)
            Tab(
                selected = item == tab,
                onClick = { onTab(item) },
                text = {
                    Row(horizontalArrangement = Arrangement.spacedBy(6.dp), verticalAlignment = Alignment.CenterVertically) {
                        Text(item.label, maxLines = 1)
                        if (amount > 0) Badge { Text(if (amount > 99) "99+" else amount.toString()) }
                    }
                },
            )
        }
    }
}

/** O conteúdo da aba escolhida, como itens da lista da tela. */
fun LazyListScope.threadContent(model: MainViewModel, view: TicketView, tab: ThreadTab) {
    if (tab == ThreadTab.Attachments) {
        if (view.attachments.isEmpty()) {
            item { Text("Nenhum anexo cadastrado.", style = MaterialTheme.typography.bodyLarge) }
        }
        items(view.attachments.size) { index -> AttachmentRow(model, view.id, view.attachments[index]) }
        return
    }
    val entries = entriesOf(view, tab)
    if (entries.isEmpty()) {
        item {
            Text(
                when (tab) {
                    ThreadTab.Comments -> "Nenhum comentário."
                    ThreadTab.Notes -> "Nenhuma nota interna cadastrada."
                    else -> "Nada na linha do tempo ainda."
                },
                style = MaterialTheme.typography.bodyLarge,
            )
        }
    }
    items(entries.size) { index -> ThreadEntryItem(entries[index], entries[index].photo?.let { model.photoFiles[it] }) }
}

@Composable
private fun AttachmentRow(model: MainViewModel, id: String, attachment: AttachmentItem) {
    Column(verticalArrangement = Arrangement.spacedBy(2.dp)) {
        OutlinedButton(
            onClick = { model.download(id, attachment.number) },
            enabled = model.downloading == null,
            modifier = Modifier.fillMaxWidth(),
        ) { Text(if (model.downloading == attachment.number) "Baixando…" else attachment.name) }
        attachment.by?.let { Text(it, style = MaterialTheme.typography.labelMedium, color = MaterialTheme.colorScheme.onSurfaceVariant) }
    }
}

/** Uma entrada: quem escreveu (foto e nome), o que foi, quando e o texto; as notas internas em destaque. */
@Composable
fun ThreadEntryItem(entry: ThreadItem, photo: String?) {
    val isNote = entry.kind == EntryKind.NOTE
    val verb = when (entry.kind) {
        EntryKind.COMMENT -> "comentou"
        EntryKind.NOTE -> "escreveu uma nota interna"
        EntryKind.ATTACHMENT -> "anexou um arquivo"
        EntryKind.EVENT -> null
    }
    Column(
        Modifier
            .fillMaxWidth()
            .clip(RoundedCornerShape(4.dp))
            .background(if (isNote) MaterialTheme.colorScheme.secondaryContainer else MaterialTheme.colorScheme.surface)
            .padding(vertical = 8.dp, horizontal = if (isNote) 8.dp else 0.dp),
        verticalArrangement = Arrangement.spacedBy(4.dp),
    ) {
        Row(horizontalArrangement = Arrangement.spacedBy(10.dp), verticalAlignment = Alignment.CenterVertically) {
            if (entry.author != null) PersonAvatar(photo, size = 32.dp)
            Column {
                if (entry.author != null) {
                    Text(
                        entry.author + (verb?.let { " $it" } ?: ""),
                        style = MaterialTheme.typography.labelLarge,
                        fontWeight = FontWeight.SemiBold,
                    )
                }
                Text(entry.date, style = MaterialTheme.typography.labelMedium, color = MaterialTheme.colorScheme.onSurfaceVariant)
            }
        }
        Text(
            entry.body,
            style = MaterialTheme.typography.bodyLarge,
            fontStyle = if (entry.kind == EntryKind.EVENT) FontStyle.Italic else FontStyle.Normal,
        )
        HorizontalDivider(Modifier.padding(top = 4.dp), color = MaterialTheme.colorScheme.outlineVariant)
    }
}
