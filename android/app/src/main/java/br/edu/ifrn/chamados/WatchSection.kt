package br.edu.ifrn.chamados

import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.material3.FilterChip
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
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
import uniffi.chamados_mobile.Queue

/** Acompanhamento em segundo plano: avisa quando um chamado muda, sem abrir o app. */
@OptIn(ExperimentalLayoutApi::class)
@Composable
fun WatchSection() {
    val context = LocalContext.current
    var settings by remember { mutableStateOf(Watcher.load(context)) }
    var note by remember { mutableStateOf<String?>(null) }

    fun update(next: WatchSettings) {
        settings = next
        Watcher.save(context, next)
    }

    val permission = rememberLauncherForActivityResult(ActivityResultContracts.RequestPermission()) { granted ->
        note = if (granted) null else "Sem a permissão de notificações o acompanhamento roda, mas não consegue avisar."
    }

    HorizontalDivider()
    Text("Acompanhamento", style = MaterialTheme.typography.titleMedium)
    Text(
        "Consulta o SUAP de tempos em tempos e avisa o que mudou (só lê, nada é enviado). " +
            "A economia de bateria do Android pode atrasar as consultas, e o intervalo mínimo é de 15 minutos.",
        style = MaterialTheme.typography.bodySmall,
    )
    Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(12.dp)) {
        Switch(
            checked = settings.enabled,
            onCheckedChange = { on ->
                update(settings.copy(enabled = on))
                if (on && !Watcher.canNotify(context)) {
                    permission.launch(android.Manifest.permission.POST_NOTIFICATIONS)
                }
            },
        )
        Text(if (settings.enabled) "Ligado" else "Desligado")
    }
    if (settings.enabled) {
        Text("A cada", style = MaterialTheme.typography.labelMedium)
        FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            Watcher.intervals.forEach { minutes ->
                val label = if (minutes < 60) "$minutes min" else "${minutes / 60} h"
                FilterChip(selected = settings.minutes == minutes, onClick = { update(settings.copy(minutes = minutes)) }, label = { Text(label) })
            }
        }
        Text("Lista", style = MaterialTheme.typography.labelMedium)
        FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            listOf(Queue.MINE to "Meus", Queue.SUPPORT to "Suporte").forEach { (queue, label) ->
                FilterChip(selected = settings.queue == queue, onClick = { update(settings.copy(queue = queue)) }, label = { Text(label) })
            }
        }
        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(12.dp)) {
            Switch(checked = settings.messages, onCheckedChange = { update(settings.copy(messages = it)) })
            Text("Avisar de mensagens novas (consulta mais pesada)")
        }
        OutlinedButton(
            onClick = {
                Watcher.checkNow(context)
                note = "Consulta pedida; se algo mudou, o aviso chega em instantes."
            },
            modifier = Modifier.fillMaxWidth(),
        ) { Text("Verificar agora") }
    }
    note?.let { Text(it, style = MaterialTheme.typography.labelMedium) }
}
