package br.edu.ifrn.chamados

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.produceState
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.unit.dp

/** Configurações do assistente de redação ("Sugerir com IA"): liga e desliga, o estado da IA do sistema e os documentos de apoio. */
@Composable
fun AssistantSection(onDocuments: () -> Unit) {
    val context = LocalContext.current
    var enabled by remember { mutableStateOf(Assistant.enabled(context)) }
    val system by produceState<AiState?>(null) { value = Assistant.systemState() }

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
            AiState.Unsupported -> "indisponível neste aparelho; o botão coloca um modelo pronto para você completar."
            null -> "verificando…"
        },
        style = MaterialTheme.typography.bodySmall,
    )

    OutlinedButton(onClick = onDocuments, modifier = Modifier.fillMaxWidth()) { Text("Documentos de apoio…") }
}
