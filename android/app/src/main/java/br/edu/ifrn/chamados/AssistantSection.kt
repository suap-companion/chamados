package br.edu.ifrn.chamados

import android.app.ActivityManager
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Checkbox
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.LinearProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.produceState
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.unit.dp
import kotlinx.coroutines.delay

/**
 * Configurações do assistente de redação ("Sugerir com IA"): liga e desliga, o estado da IA do sistema e o
 * modelo local de reserva, que só é baixado com o consentimento da pessoa (tamanho, licença, origem e espaço
 * ficam claros antes) e pode ser apagado a qualquer hora.
 */
@Composable
fun AssistantSection(onDocuments: () -> Unit) {
    val context = LocalContext.current
    val downloader = remember { ModelDownloader(context) }
    var enabled by remember { mutableStateOf(Assistant.enabled(context)) }
    var state by remember { mutableStateOf(downloader.state()) }
    var consent by remember { mutableStateOf(false) }
    var deleting by remember { mutableStateOf(false) }
    val system by produceState<AiState?>(null) { value = Assistant.systemState() }

    // Acompanha o download (feito pelo sistema) e, ao terminar, confere o SHA-256.
    LaunchedEffect(Unit) {
        while (true) {
            state = downloader.state()
            if (state is ModelState.Verifying) state = downloader.verify()
            delay(1000)
        }
    }

    HorizontalDivider()
    Text("Assistente de IA", style = MaterialTheme.typography.titleMedium)
    Text(
        "O botão \"Sugerir com IA\" escreve um rascunho de comentário, nota, resolução e título a partir do chamado aberto. " +
            "Tudo acontece no aparelho; nada é enviado ao SUAP sem a sua confirmação.",
        style = MaterialTheme.typography.bodySmall,
    )
    Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(12.dp)) {
        Switch(checked = enabled, onCheckedChange = { enabled = it; Assistant.setEnabled(context, it) })
        Text(if (enabled) "Ligado" else "Desligado")
    }
    Text(
        "IA do sistema (Gemini Nano): " + when (system) {
            AiState.Ready -> "pronta."
            AiState.Downloadable -> "ainda não baixada (o botão do diálogo oferece o download)."
            AiState.Downloading -> "baixando."
            AiState.Unsupported -> "indisponível neste aparelho."
            null -> "verificando…"
        },
        style = MaterialTheme.typography.bodySmall,
    )

    OutlinedButton(onClick = onDocuments, modifier = Modifier.fillMaxWidth()) { Text("Documentos de apoio…") }

    Text("Modelo local de reserva (opcional)", style = MaterialTheme.typography.labelLarge)
    Text(
        "${LocalModel.NAME}, ${formatSize(LocalModel.SPEC.sizeBytes)}, licença ${LocalModel.LICENSE}. Roda no aparelho " +
            "quando a IA do sistema não está disponível; a qualidade é menor que a da IA do sistema.",
        style = MaterialTheme.typography.bodySmall,
    )
    when (val current = state) {
        ModelState.Absent -> OutlinedButton(onClick = { consent = true }, modifier = Modifier.fillMaxWidth()) {
            Text("Baixar o modelo (${formatSize(LocalModel.SPEC.sizeBytes)})…")
        }
        is ModelState.Downloading -> {
            Text("Baixando… ${formatSize(current.done)} de ${formatSize(current.total)}", style = MaterialTheme.typography.bodySmall)
            LinearProgressIndicator(progress = { (current.done.toFloat() / current.total).coerceIn(0f, 1f) }, modifier = Modifier.fillMaxWidth())
            OutlinedButton(onClick = { downloader.remove(); state = downloader.state() }, modifier = Modifier.fillMaxWidth()) {
                Text("Cancelar o download")
            }
        }
        is ModelState.Paused -> {
            Text(current.reason, style = MaterialTheme.typography.bodySmall)
            OutlinedButton(onClick = { downloader.remove(); state = downloader.state() }, modifier = Modifier.fillMaxWidth()) {
                Text("Cancelar o download")
            }
        }
        ModelState.Verifying -> Text("Conferindo o arquivo baixado (SHA-256)…", style = MaterialTheme.typography.bodySmall)
        is ModelState.Failed -> {
            Text(current.message, color = MaterialTheme.colorScheme.error, style = MaterialTheme.typography.bodySmall)
            OutlinedButton(onClick = { consent = true }, modifier = Modifier.fillMaxWidth()) { Text("Tentar de novo…") }
        }
        ModelState.Installed -> {
            Text("Instalado (${formatSize(LocalModel.SPEC.sizeBytes)}).", style = MaterialTheme.typography.bodySmall)
            OutlinedButton(onClick = { deleting = true }, modifier = Modifier.fillMaxWidth()) {
                Text("Apagar o modelo (libera ${formatSize(LocalModel.SPEC.sizeBytes)})")
            }
        }
    }

    if (consent) {
        ConsentDialog(
            free = downloader.freeBytes(),
            onDismiss = { consent = false },
            onConfirm = { wifiOnly ->
                consent = false
                downloader.start(wifiOnly)
                state = downloader.state()
            },
        )
    }
    if (deleting) {
        AlertDialog(
            onDismissRequest = { deleting = false },
            title = { Text("Apagar o modelo local?") },
            text = { Text("O arquivo de ${formatSize(LocalModel.SPEC.sizeBytes)} sai do aparelho. Você pode baixá-lo de novo quando quiser.") },
            confirmButton = {
                TextButton(onClick = { deleting = false; downloader.remove(); state = downloader.state() }) { Text("Apagar") }
            },
            dismissButton = { TextButton(onClick = { deleting = false }) { Text("Voltar") } },
        )
    }
}

/** O consentimento do download: o que é, o tamanho, a licença, a origem e o espaço, antes de qualquer byte. */
@Composable
private fun ConsentDialog(free: Long, onDismiss: () -> Unit, onConfirm: (wifiOnly: Boolean) -> Unit) {
    val context = LocalContext.current
    var wifiOnly by remember { mutableStateOf(true) }
    val totalRam = remember {
        ActivityManager.MemoryInfo().also { context.getSystemService(ActivityManager::class.java).getMemoryInfo(it) }.totalMem
    }
    val size = LocalModel.SPEC.sizeBytes
    val space = enoughSpace(free, size)
    val ram = totalRam >= LocalModel.MIN_RAM_BYTES
    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text("Baixar o modelo local?") },
        text = {
            androidx.compose.foundation.layout.Column(
                verticalArrangement = Arrangement.spacedBy(8.dp),
                modifier = Modifier.fillMaxWidth(),
            ) {
                Text("${LocalModel.NAME}: um modelo de linguagem que roda no aparelho, sem enviar nada para fora.")
                Text("Tamanho: ${formatSize(size)} (fica no armazenamento do app e sai com ele ou com \"Apagar\").")
                Text("Licença: ${LocalModel.LICENSE}. Origem: ${LocalModel.SOURCE}.")
                Text("Espaço livre: ${formatSize(free)}" + if (space) "." else " (insuficiente: libere espaço).",
                    color = if (space) MaterialTheme.colorScheme.onSurface else MaterialTheme.colorScheme.error)
                if (!ram) {
                    Text(
                        "Este aparelho tem ${formatSize(totalRam)} de memória; o modelo precisa de pelo menos ${formatSize(LocalModel.MIN_RAM_BYTES)} " +
                            "e pode não funcionar.",
                        color = MaterialTheme.colorScheme.error,
                    )
                }
                Row(verticalAlignment = Alignment.CenterVertically) {
                    Checkbox(checked = wifiOnly, onCheckedChange = { wifiOnly = it })
                    Text("Baixar só no Wi-Fi")
                }
                Text("O arquivo é conferido pelo SHA-256 publicado antes de ser usado.", style = MaterialTheme.typography.bodySmall)
            }
        },
        confirmButton = { TextButton(onClick = { onConfirm(wifiOnly) }, enabled = space) { Text("Baixar") } },
        dismissButton = { TextButton(onClick = onDismiss) { Text("Cancelar") } },
    )
}
