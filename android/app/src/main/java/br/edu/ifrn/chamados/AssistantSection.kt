package br.edu.ifrn.chamados

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.FilterChip
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.ui.text.input.PasswordVisualTransformation
import kotlinx.coroutines.launch
import uniffi.chamados_mobile.CloudProviderInfo
import uniffi.chamados_mobile.cloudGenerate
import uniffi.chamados_mobile.cloudProviders
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

    CloudSection()

    OutlinedButton(onClick = onDocuments, modifier = Modifier.fillMaxWidth()) { Text("Documentos de apoio…") }
}

/**
 * A IA na nuvem, opcional: a pessoa escolhe o provedor, informa a chave de API **dela** (guardada cifrada, só
 * neste aparelho) e liga depois de ler para onde o texto vai. Desligada por padrão.
 */
@OptIn(ExperimentalLayoutApi::class)
@Composable
private fun CloudSection() {
    val context = LocalContext.current
    val scope = rememberCoroutineScope()
    val providers = remember { cloudProviders() }
    var enabled by remember { mutableStateOf(CloudAi.enabled(context)) }
    var provider by remember { mutableStateOf(CloudAi.provider(context)) }
    val info: CloudProviderInfo = providers.firstOrNull { it.name == provider } ?: providers.first()
    var model by remember(provider) { mutableStateOf(CloudAi.model(context, provider).orEmpty()) }
    var address by remember(provider) { mutableStateOf(CloudAi.baseUrl(context, provider).orEmpty()) }
    var key by remember { mutableStateOf("") }
    var stored by remember(provider) { mutableStateOf(CloudAi.vault(context).load(provider)) }
    var asking by remember { mutableStateOf(false) }
    var testing by remember { mutableStateOf(false) }
    var message by remember { mutableStateOf<String?>(null) }

    HorizontalDivider()
    Text("IA na nuvem (opcional)", style = MaterialTheme.typography.titleMedium)
    Text(
        "Para aparelhos sem IA do sistema: use a sua própria conta de um provedor. O texto do chamado aberto vai ao provedor " +
            "que você escolher; por isso fica desligada até você ligar e confirmar.",
        style = MaterialTheme.typography.bodySmall,
    )
    Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(12.dp)) {
        Switch(
            checked = enabled,
            onCheckedChange = { wanted -> if (wanted) asking = true else { enabled = false; CloudAi.setEnabled(context, false) } },
        )
        Text(if (enabled) "Ligada" else "Desligada")
    }

    FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
        providers.forEach { option ->
            FilterChip(
                selected = provider == option.name,
                onClick = { provider = option.name; message = null },
                label = { Text(option.label) },
            )
        }
    }
    OutlinedTextField(
        value = model, onValueChange = { model = it }, singleLine = true, modifier = Modifier.fillMaxWidth(),
        label = { Text("Modelo (vazio: ${info.defaultModel})") },
    )
    if (info.customUrl) {
        OutlinedTextField(
            value = address, onValueChange = { address = it }, singleLine = true, modifier = Modifier.fillMaxWidth(),
            label = { Text("Endereço da API (vazio: ${info.defaultBaseUrl})") },
        )
    }
    OutlinedTextField(
        value = key, onValueChange = { key = it }, singleLine = true, modifier = Modifier.fillMaxWidth(),
        visualTransformation = PasswordVisualTransformation(),
        label = { Text(if (stored != null) "Nova chave de API" else "Chave de API") },
    )
    Text(
        stored?.let { "Chave guardada: ${maskKey(it)} (cifrada, só neste aparelho)." } ?: "Nenhuma chave guardada.",
        style = MaterialTheme.typography.bodySmall,
    )
    Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
        OutlinedButton(
            enabled = key.isNotBlank() || stored != null,
            onClick = {
                CloudAi.save(context, provider, model, address)
                if (key.isNotBlank()) {
                    CloudAi.vault(context).save(provider, key.trim())
                    stored = key.trim()
                    key = ""
                }
                message = "Configuração guardada."
            },
        ) { Text("Guardar") }
        OutlinedButton(
            enabled = stored != null && !testing,
            onClick = {
                testing = true
                message = "Testando…"
                scope.launch {
                    message = try {
                        cloudGenerate(provider, model.ifBlank { null }, address.ifBlank { null }, stored.orEmpty(), "Responda apenas: ok")
                        "Conexão ok."
                    } catch (error: Throwable) {
                        error.assistMessage()
                    }
                    testing = false
                }
            },
        ) { Text("Testar conexão") }
        TextButton(
            enabled = stored != null,
            onClick = {
                CloudAi.vault(context).clear(provider)
                stored = null
                message = "Chave apagada deste aparelho."
            },
        ) { Text("Apagar a chave") }
    }
    message?.let { Text(it, style = MaterialTheme.typography.bodySmall) }

    if (asking) {
        val host = hostOf(address, info.defaultBaseUrl)
        AlertDialog(
            onDismissRequest = { asking = false },
            title = { Text("Ligar a IA na nuvem?") },
            text = {
                Text(
                    "Ao pedir um rascunho, o app envia a $host: o título, a descrição e as mensagens do chamado aberto (as notas " +
                        "internas só nos textos internos, como nota interna e escalar) e os trechos dos documentos de apoio que " +
                        "ajudarem. Nada é enviado ao SUAP sem a sua confirmação, e as credenciais e a sessão do SUAP nunca saem " +
                        "do aparelho.\n\nA chave é sua: fica cifrada só neste aparelho e o uso entra na cota e na cobrança da sua " +
                        "conta no provedor. Você pode desligar e apagar a chave quando quiser.",
                )
            },
            confirmButton = {
                TextButton(onClick = { asking = false; enabled = true; CloudAi.setEnabled(context, true) }) { Text("Entendi, ligar") }
            },
            dismissButton = { TextButton(onClick = { asking = false }) { Text("Cancelar") } },
        )
    }
}
