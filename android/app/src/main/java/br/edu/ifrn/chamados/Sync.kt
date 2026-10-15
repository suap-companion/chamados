package br.edu.ifrn.chamados

import androidx.activity.compose.BackHandler
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Button
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.FilterChip
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.TopAppBar
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.input.PasswordVisualTransformation
import androidx.compose.ui.unit.dp
import uniffi.chamados_mobile.SyncSetup

/** Sincronização cifrada de títulos e perfis entre o app e o computador (mesmo bucket e mesma chave). */
@OptIn(ExperimentalMaterial3Api::class, ExperimentalLayoutApi::class)
@Composable
fun SyncScreen(model: MainViewModel) {
    BackHandler { model.back() }
    val state = model.syncState
    var scanning by remember { mutableStateOf(false) }
    var confirm by remember { mutableStateOf<Pair<String, () -> Unit>?>(null) }
    var pasted by remember { mutableStateOf("") }
    var backend by remember(state?.backend) { mutableStateOf(state?.backend ?: "r2") }
    var endpoint by remember(state?.endpoint) { mutableStateOf(state?.endpoint.orEmpty()) }
    var bucket by remember(state?.bucket) { mutableStateOf(state?.bucket.orEmpty()) }
    var region by remember(state?.region) { mutableStateOf(state?.region.orEmpty()) }
    var prefix by remember(state?.prefix) { mutableStateOf(state?.prefix.orEmpty()) }
    var accessKey by remember { mutableStateOf("") }
    var secret by remember { mutableStateOf("") }
    var hex by remember { mutableStateOf("") }
    val hasKey = state?.hasKey == true

    /** Pede confirmação antes de trocar uma chave que já existe (o que ela cifrou ficaria ilegível). */
    fun replacing(action: (Boolean) -> Unit) {
        if (hasKey) {
            confirm = "Já existe uma chave neste aparelho. Trocá-la torna ilegível o que foi cifrado com a antiga." to { action(true) }
        } else {
            action(false)
        }
    }

    if (scanning) {
        BackHandler { scanning = false }
        QrScanner(
            onResult = { text ->
                scanning = false
                replacing { replace -> model.applyPairing(text, replace) }
            },
            onCancel = { scanning = false },
        )
        return
    }
    confirm?.let { (message, action) ->
        AlertDialog(
            onDismissRequest = { confirm = null },
            title = { Text("Trocar a chave?") },
            text = { Text(message) },
            confirmButton = { TextButton(onClick = { confirm = null; action() }) { Text("Trocar") } },
            dismissButton = { TextButton(onClick = { confirm = null }) { Text("Voltar") } },
        )
    }
    Scaffold(
        topBar = { GovBrHeader("Sincronização", onBack = model::back) },
    ) { padding ->
        Column(
            Modifier.padding(padding).padding(16.dp).verticalScroll(rememberScrollState()),
            verticalArrangement = Arrangement.spacedBy(12.dp),
        ) {
            Text(
                "Títulos locais e perfis seguem cifrados (só você tem a chave) para um bucket seu. " +
                    "Rodar \"chamados sync key pair\" no computador mostra o QR que configura tudo, menos as credenciais do bucket.",
            )
            if (state != null) {
                val bucketText = if (state.configured) "${state.backend}: ${state.bucket}" + (state.prefix?.let { "/$it" } ?: "") else "não configurado"
                Text("Bucket: $bucketText", style = MaterialTheme.typography.labelLarge)
                Text("Chave: ${if (state.hasKey) "sim" else "não"} · Credenciais: ${if (state.hasCredentials) "sim" else "não"}", style = MaterialTheme.typography.labelLarge)
            }
            model.syncMessage?.let {
                Text(it, color = if (model.syncIsError) MaterialTheme.colorScheme.error else MaterialTheme.colorScheme.primary)
            }
            if (model.syncBusy) CircularProgressIndicator()

            Button(
                onClick = { model.syncNow() },
                enabled = !model.syncBusy && state?.configured == true && hasKey && state.hasCredentials,
                modifier = Modifier.fillMaxWidth(),
            ) { Text("Sincronizar agora") }

            HorizontalDivider()
            Text("Parear com o computador", style = MaterialTheme.typography.titleMedium)
            Button(onClick = { scanning = true }, enabled = !model.syncBusy, modifier = Modifier.fillMaxWidth()) { Text("Ler QR") }
            OutlinedTextField(
                value = pasted,
                onValueChange = { pasted = it },
                label = { Text("ou cole o texto do pareamento") },
                modifier = Modifier.fillMaxWidth(),
            )
            OutlinedButton(
                onClick = { replacing { replace -> model.applyPairing(pasted, replace) } },
                enabled = !model.syncBusy && pasted.isNotBlank(),
                modifier = Modifier.fillMaxWidth(),
            ) { Text("Aplicar texto") }

            HorizontalDivider()
            Text("Credenciais do bucket", style = MaterialTheme.typography.titleMedium)
            OutlinedTextField(
                value = accessKey,
                onValueChange = { accessKey = it },
                label = { Text("Access Key ID") },
                singleLine = true,
                modifier = Modifier.fillMaxWidth(),
            )
            OutlinedTextField(
                value = secret,
                onValueChange = { secret = it },
                label = { Text("Secret Access Key") },
                singleLine = true,
                visualTransformation = PasswordVisualTransformation(),
                modifier = Modifier.fillMaxWidth(),
            )
            OutlinedButton(
                onClick = {
                    model.saveCredentials(accessKey, secret)
                    accessKey = ""
                    secret = ""
                },
                enabled = !model.syncBusy && accessKey.isNotBlank() && secret.isNotBlank(),
                modifier = Modifier.fillMaxWidth(),
            ) { Text("Guardar credenciais") }

            HorizontalDivider()
            Text("Chave de criptografia", style = MaterialTheme.typography.titleMedium)
            FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                OutlinedButton(onClick = { replacing { replace -> model.generateKey(replace) } }, enabled = !model.syncBusy) { Text("Criar chave") }
                OutlinedButton(onClick = model::revealKey, enabled = !model.syncBusy && hasKey) { Text("Mostrar chave") }
            }
            model.shownKey?.let {
                Text("Guarde em segredo; leve-a ao computador com \"chamados sync key import\".", style = MaterialTheme.typography.labelMedium)
                SelectionContainer { Text(it, style = MaterialTheme.typography.bodySmall) }
            }
            OutlinedTextField(
                value = hex,
                onValueChange = { hex = it },
                label = { Text("Chave vinda do computador (64 hexadecimais)") },
                modifier = Modifier.fillMaxWidth(),
            )
            OutlinedButton(
                onClick = { replacing { replace -> model.importKey(hex, replace) } },
                enabled = !model.syncBusy && hex.isNotBlank(),
                modifier = Modifier.fillMaxWidth(),
            ) { Text("Guardar chave") }

            HorizontalDivider()
            Text("Bucket (sem QR)", style = MaterialTheme.typography.titleMedium)
            FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                listOf("r2" to "Cloudflare R2", "s3" to "S3").forEach { (name, label) ->
                    FilterChip(selected = backend == name, onClick = { backend = name }, label = { Text(label) })
                }
            }
            OutlinedTextField(value = endpoint, onValueChange = { endpoint = it }, label = { Text("Endpoint (https://…)") }, singleLine = true, modifier = Modifier.fillMaxWidth())
            OutlinedTextField(value = bucket, onValueChange = { bucket = it }, label = { Text("Bucket") }, singleLine = true, modifier = Modifier.fillMaxWidth())
            OutlinedTextField(value = region, onValueChange = { region = it }, label = { Text("Região (vazio no R2)") }, singleLine = true, modifier = Modifier.fillMaxWidth())
            OutlinedTextField(value = prefix, onValueChange = { prefix = it }, label = { Text("Prefixo (opcional)") }, singleLine = true, modifier = Modifier.fillMaxWidth())
            OutlinedButton(
                onClick = {
                    model.saveBucket(SyncSetup(backend, endpoint.trim(), bucket.trim(), region.trim().ifEmpty { null }, prefix.trim().ifEmpty { null }))
                },
                enabled = !model.syncBusy && endpoint.isNotBlank() && bucket.isNotBlank(),
                modifier = Modifier.fillMaxWidth(),
            ) { Text("Guardar bucket") }

            HorizontalDivider()
            OutlinedButton(onClick = model::forgetSync, enabled = !model.syncBusy && state?.configured == true, modifier = Modifier.fillMaxWidth()) {
                Text("Desligar neste aparelho (apaga chave e credenciais)")
            }
        }
    }
}
