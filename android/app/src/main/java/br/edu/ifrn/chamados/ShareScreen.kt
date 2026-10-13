package br.edu.ifrn.chamados

import android.app.Activity
import android.content.Intent
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Button
import androidx.compose.material3.Card
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.RadioButton
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.TopAppBar
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.style.TextDecoration
import androidx.compose.ui.unit.dp
import androidx.compose.ui.graphics.Color

/** A tela "O que fazer com isto?": mostra o que chegou e SEMPRE pergunta a ação e o chamado. */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun ShareScreen(model: ShareViewModel) {
    val activity = LocalContext.current as Activity
    Scaffold(topBar = { TopAppBar(title = { Text("Enviar para o SUAP") }) }) { padding ->
        Column(Modifier.padding(padding).fillMaxSize()) {
            when (model.phase) {
                SharePhase.Loading, SharePhase.Sending -> Column(
                    Modifier.fillMaxSize(),
                    verticalArrangement = Arrangement.Center,
                    horizontalAlignment = Alignment.CenterHorizontally,
                ) { CircularProgressIndicator() }

                SharePhase.NeedsLogin -> NeedsLogin(model, activity)
                SharePhase.Choosing -> Choosing(model)
                SharePhase.Finished -> Finished(model, activity)
            }
        }
    }
}

@Composable
private fun NeedsLogin(model: ShareViewModel, activity: Activity) {
    Column(Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
        Text("Você precisa entrar no SUAP antes de enviar. O que você compartilhou fica guardado aqui.")
        Button(onClick = { activity.startActivity(Intent(activity, MainActivity::class.java)) }, modifier = Modifier.fillMaxWidth()) {
            Text("Abrir o app para entrar")
        }
        OutlinedButton(onClick = model::checkSession, modifier = Modifier.fillMaxWidth()) { Text("Já entrei, continuar") }
        TextButton(onClick = { model.cleanup(); activity.finish() }, modifier = Modifier.fillMaxWidth()) { Text("Descartar") }
    }
}

@Composable
private fun Choosing(model: ShareViewModel) {
    LazyColumn(contentPadding = PaddingValues(16.dp), verticalArrangement = Arrangement.spacedBy(10.dp)) {
        model.error?.let { item { Text(it, color = MaterialTheme.colorScheme.error) } }

        item { Text("O que chegou", style = MaterialTheme.typography.titleMedium) }
        items(model.files) { Text("• ${it.name} (${readableSize(it.size)})") }
        items(model.rejected) {
            Column {
                Text("• ${it.name}", textDecoration = TextDecoration.LineThrough, color = MaterialTheme.colorScheme.error)
                Text(it.reason, style = MaterialTheme.typography.labelMedium, color = MaterialTheme.colorScheme.error)
            }
        }
        item {
            OutlinedTextField(
                value = model.text,
                onValueChange = { model.text = it },
                label = { Text("Texto (descrição, mensagem ou motivo)") },
                minLines = 3,
                modifier = Modifier.fillMaxWidth(),
            )
        }

        item { Text("O que fazer?", style = MaterialTheme.typography.titleMedium) }
        items(ShareChoice.entries) { option ->
            val reason = model.unavailable(option)
            Row(
                Modifier.fillMaxWidth().clickable(enabled = reason == null) { model.pick(option) },
                verticalAlignment = Alignment.CenterVertically,
            ) {
                RadioButton(selected = model.choice == option, onClick = { model.pick(option) }, enabled = reason == null)
                Column {
                    Text(option.label, color = if (reason == null) Color.Unspecified else MaterialTheme.colorScheme.outline)
                    reason?.let { Text(it, style = MaterialTheme.typography.labelMedium, color = MaterialTheme.colorScheme.outline) }
                }
            }
        }

        val option = model.choice
        if (option == ShareChoice.Create) {
            item {
                OutlinedTextField(
                    value = model.serviceId,
                    onValueChange = { model.serviceId = it },
                    label = { Text("Número do serviço") },
                    singleLine = true,
                    modifier = Modifier.fillMaxWidth(),
                )
                OutlinedTextField(
                    value = model.interested,
                    onValueChange = { model.interested = it },
                    label = { Text("Interessado (vínculo; opcional)") },
                    singleLine = true,
                    modifier = Modifier.fillMaxWidth(),
                )
            }
        }
        if (option != null && option.needsTicket) {
            item {
                OutlinedTextField(
                    value = model.ticket,
                    onValueChange = { model.pickTicket(it) },
                    label = { Text("Número do chamado") },
                    singleLine = true,
                    modifier = Modifier.fillMaxWidth(),
                )
            }
            val shown = model.tickets.filter { model.ticket.isBlank() || it.id.contains(model.ticket.trim()) }.take(8)
            items(shown, key = { it.id }) { ticket ->
                Card(Modifier.fillMaxWidth().clickable { model.pickTicket(ticket.id) }) {
                    Column(Modifier.padding(10.dp)) {
                        Text("#${ticket.id}  ${ticket.status.orEmpty()}", style = MaterialTheme.typography.labelLarge)
                        Text(ticket.subject ?: "(sem assunto)")
                    }
                }
            }
        }

        model.outcome?.let { result ->
            item { Text("Resultado", style = MaterialTheme.typography.titleMedium) }
            items(result.steps) { step ->
                val mark = if (step.error == null) "✓" else "✗"
                Text("$mark ${step.label}${step.error?.let { ": $it" } ?: ""}")
            }
        }

        item {
            val missing = model.missing()
            val retrying = model.outcome != null
            Column(verticalArrangement = Arrangement.spacedBy(6.dp)) {
                missing?.let { Text(it, style = MaterialTheme.typography.labelMedium) }
                Button(onClick = { model.confirming = true }, enabled = missing == null, modifier = Modifier.fillMaxWidth()) {
                    Text(if (retrying) "Tentar de novo só o que falhou" else "Revisar e enviar")
                }
            }
        }
    }
    if (model.confirming) Confirm(model)
}

@Composable
private fun Confirm(model: ShareViewModel) {
    val option = model.choice ?: return
    val where = if (option == ShareChoice.Create) "novo chamado (serviço ${model.serviceId.trim()})" else "chamado #${model.ticket.trim()}"
    val extra = if (option == ShareChoice.Cancel) "\n\nCancelar não pode ser desfeito." else ""
    val attached = when {
        option == ShareChoice.Attach && model.text.isNotBlank() && model.files.isEmpty() -> "o texto como PDF"
        option == ShareChoice.Attach && model.text.isNotBlank() -> "${model.files.size} arquivo(s) e o texto como PDF"
        option == ShareChoice.Attach -> "${model.files.size} arquivo(s)"
        model.files.isNotEmpty() -> "${model.files.size} arquivo(s) anexado(s) antes da mensagem"
        else -> "sem arquivos"
    }
    AlertDialog(
        onDismissRequest = { model.confirming = false },
        title = { Text(option.label) },
        text = { Text("Destino: $where\nAnexos: $attached$extra") },
        confirmButton = { TextButton(onClick = model::send) { Text(if (option == ShareChoice.Cancel) "Cancelar o chamado" else "Enviar") } },
        dismissButton = { TextButton(onClick = { model.confirming = false }) { Text("Voltar") } },
    )
}

@Composable
private fun Finished(model: ShareViewModel, activity: Activity) {
    Column(Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
        Text("Pronto!", style = MaterialTheme.typography.titleLarge)
        model.outcome?.steps?.forEach { Text("✓ ${it.label}") }
        model.outcome?.ticket?.let { Text("Chamado #$it") }
        Button(onClick = { activity.finish() }, modifier = Modifier.fillMaxWidth()) { Text("Fechar") }
    }
}

private fun readableSize(bytes: Long): String =
    if (bytes < 1024) "$bytes bytes" else String.format(java.util.Locale.US, "%.1f MB", bytes / 1048576.0).let {
        if (bytes < 1048576) "${bytes / 1024} KB" else it
    }
