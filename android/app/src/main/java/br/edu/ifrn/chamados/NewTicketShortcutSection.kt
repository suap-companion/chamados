package br.edu.ifrn.chamados

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.Column
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.unit.dp

/** Configurações: abrir "Novo chamado" direto no passo 3 quando o serviço e o interessado padrão já existem. */
@Composable
fun NewTicketShortcutSection() {
    val context = LocalContext.current
    var enabled by remember { mutableStateOf(CoreAccess.newTicketShortcut(context)) }
    Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(12.dp)) {
        Column(Modifier.weight(1f)) {
            Text("Atalho do novo chamado", style = MaterialTheme.typography.titleSmall)
            Text(
                "Com serviço e interessado padrão, o + abre direto na descrição.",
                style = MaterialTheme.typography.labelMedium,
            )
        }
        Switch(checked = enabled, onCheckedChange = { enabled = it; CoreAccess.saveNewTicketShortcut(context, it) })
    }
}
