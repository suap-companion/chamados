package br.edu.ifrn.chamados

import androidx.activity.compose.BackHandler
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Button
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.FilterChip
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.TopAppBar
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateListOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import uniffi.chamados_mobile.TicketView

/** Um campo de uma ação de gestão. */
data class ManageField(val label: String, val required: Boolean, val lines: Int = 1)

/** As ações de gestão do chamado: nome ou matrícula onde há pessoas, vírgulas onde há listas. */
enum class ManageAction(val label: String, val fields: List<ManageField>) {
    Assign("Atribuir a outro atendente", listOf(ManageField("Nome ou matrícula do atendente", true))),
    Escalate(
        "Escalar",
        listOf(ManageField("Nota interna", true, 3), ManageField("Atribuir a (opcional)", false)),
    ),
    Return(
        "Devolver",
        listOf(ManageField("Nota interna", true, 3), ManageField("Atribuir a (opcional)", false)),
    ),
    Reclassify(
        "Reclassificar",
        listOf(
            ManageField("Serviço (opcional)", false),
            ManageField("Campus (opcional)", false),
            ManageField("Centro (opcional)", false),
            ManageField("Justificativa", true, 3),
        ),
    ),
    AddTags("Adicionar tags", listOf(ManageField("Tags (separe por vírgula)", true))),
    RemoveTags("Remover tags", listOf(ManageField("Tags (separe por vírgula)", true))),
    AddInterested("Incluir interessados", listOf(ManageField("Matrículas ou nomes (separe por vírgula)", true))),
    RemoveInterested("Remover interessados", listOf(ManageField("Matrículas ou nomes (separe por vírgula)", true))),
}

/** As ações do chamado, conforme a situação dele; cada uma pede confirmação antes de enviar. */
@OptIn(ExperimentalLayoutApi::class)
@Composable
fun ActionsSection(model: MainViewModel, view: TicketView) {
    var pending by remember(view.id) { mutableStateOf<DetailAction?>(null) }
    var managing by remember(view.id) { mutableStateOf<ManageAction?>(null) }
    val actions = DetailAction.available(view.statuses)
    Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
        Text("Ações", style = MaterialTheme.typography.titleMedium)
        model.actionMessage?.let {
            Text(it, color = if (model.actionIsError) MaterialTheme.colorScheme.error else MaterialTheme.colorScheme.primary)
        }
        if (actions.isEmpty()) {
            Text("Este chamado está encerrado: não há ações.")
        } else {
            FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                actions.forEach { action ->
                    OutlinedButton(onClick = { model.dismissAction(); pending = action }, enabled = !model.actionBusy) {
                        Text(action.label)
                    }
                }
            }
        }
        if (actions.isNotEmpty()) {
            Text("Gestão", style = MaterialTheme.typography.titleMedium)
            FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                ManageAction.entries.forEach { action ->
                    OutlinedButton(onClick = { model.dismissAction(); managing = action }, enabled = !model.actionBusy) {
                        Text(action.label)
                    }
                }
            }
        }
        if (model.actionBusy) CircularProgressIndicator()
    }
    managing?.let { action ->
        ManageDialog(
            action = action,
            onDismiss = { managing = null },
            onConfirm = { values ->
                managing = null
                model.runManage(view.id, action, values)
            },
        )
    }
    pending?.let { action ->
        ActionDialog(
            model = model,
            action = action,
            onDismiss = { model.clearPicked(); pending = null },
            onConfirm = { text, rating ->
                pending = null
                model.runAction(view.id, action, text, rating)
            },
        )
    }
}

@OptIn(ExperimentalLayoutApi::class)
@Composable
private fun ActionDialog(
    model: MainViewModel,
    action: DetailAction,
    onDismiss: () -> Unit,
    onConfirm: (String?, Int?) -> Unit,
) {
    var text by remember { mutableStateOf("") }
    var rating by remember { mutableStateOf<Int?>(null) }
    val picker = rememberLauncherForActivityResult(ActivityResultContracts.GetMultipleContents()) { uris ->
        if (uris.isNotEmpty()) model.onPicked(uris, null)
    }
    val needsFiles = action == DetailAction.Attach
    val ready = when {
        action.needsText && text.isBlank() -> false
        needsFiles && model.picked.isEmpty() -> false
        else -> true
    }
    val cancelling = action == DetailAction.Cancel
    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text(action.label) },
        text = {
            Column(Modifier.verticalScroll(rememberScrollState()), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                if (action.needsText || action == DetailAction.Close) {
                    OutlinedTextField(
                        value = text,
                        onValueChange = { text = it },
                        label = { Text(if (action == DetailAction.Close) "Comentário da avaliação (opcional)" else "Texto (mensagem ou motivo)") },
                        minLines = 3,
                        modifier = Modifier.fillMaxWidth(),
                    )
                }
                if (action == DetailAction.Close) {
                    Text("Nota do atendimento")
                    FlowRow(horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                        (1..5).forEach { value ->
                            FilterChip(selected = rating == value, onClick = { rating = value }, label = { Text(value.toString()) })
                        }
                    }
                }
                if (action.takesFiles) {
                    OutlinedButton(onClick = { picker.launch("*/*") }, modifier = Modifier.fillMaxWidth()) {
                        Text(if (needsFiles) "Escolher arquivos" else "Anexar arquivos (opcional)")
                    }
                    model.picked.forEach { Text("• ${it.name}") }
                    model.pickNote?.let { Text(it, color = MaterialTheme.colorScheme.error, style = MaterialTheme.typography.labelMedium) }
                }
                if (!action.needsText && !action.takesFiles && action != DetailAction.Close) Text("Confirma esta ação no chamado #${model.detail?.id ?: ""}?")
                if (cancelling) Text("Cancelar não pode ser desfeito.", color = MaterialTheme.colorScheme.error)
            }
        },
        confirmButton = {
            TextButton(onClick = { onConfirm(text, rating) }, enabled = ready) {
                Text(if (cancelling) "Cancelar o chamado" else "Enviar")
            }
        },
        dismissButton = { TextButton(onClick = onDismiss) { Text("Voltar") } },
    )
}

@Composable
private fun ManageDialog(action: ManageAction, onDismiss: () -> Unit, onConfirm: (List<String>) -> Unit) {
    val values = remember { mutableStateListOf(*Array(action.fields.size) { "" }) }
    val ready = action.fields.indices.all { !action.fields[it].required || values[it].isNotBlank() } &&
        (action != ManageAction.Reclassify || values.take(3).any { it.isNotBlank() })
    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text(action.label) },
        text = {
            Column(Modifier.verticalScroll(rememberScrollState()), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                action.fields.forEachIndexed { index, field ->
                    OutlinedTextField(
                        value = values[index],
                        onValueChange = { values[index] = it },
                        label = { Text(field.label) },
                        minLines = field.lines,
                        singleLine = field.lines == 1,
                        modifier = Modifier.fillMaxWidth(),
                    )
                }
                if (action == ManageAction.Reclassify) Text("Preencha ao menos um entre serviço, campus e centro.")
            }
        },
        confirmButton = { TextButton(onClick = { onConfirm(values.toList()) }, enabled = ready) { Text("Enviar") } },
        dismissButton = { TextButton(onClick = onDismiss) { Text("Voltar") } },
    )
}

/** Abrir um chamado: o número do serviço (lembrado), a descrição e, se quiser, anexos. */
@OptIn(ExperimentalMaterial3Api::class, ExperimentalLayoutApi::class)
@Composable
fun NewTicketScreen(model: MainViewModel) {
    BackHandler { model.back() }
    var text by remember { mutableStateOf("") }
    val picker = rememberLauncherForActivityResult(ActivityResultContracts.GetMultipleContents()) { uris ->
        if (uris.isNotEmpty()) model.onPicked(uris, 3u)
    }
    Scaffold(
        topBar = {
            TopAppBar(
                title = { Text("Novo chamado") },
                navigationIcon = { TextButton(onClick = model::back) { Text("‹ Voltar") } },
            )
        },
    ) { padding ->
        Column(
            Modifier.padding(padding).padding(16.dp).fillMaxSize().verticalScroll(rememberScrollState()),
            verticalArrangement = Arrangement.spacedBy(12.dp),
        ) {
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
                label = { Text("Interessado (vínculo; deixe vazio se o serviço não pedir)") },
                singleLine = true,
                modifier = Modifier.fillMaxWidth(),
            )
            OutlinedTextField(
                value = text,
                onValueChange = { text = it },
                label = { Text("Descrição") },
                minLines = 5,
                modifier = Modifier.fillMaxWidth(),
            )
            Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                OutlinedButton(onClick = { picker.launch("*/*") }) { Text("Anexar (até 3)") }
                if (model.picked.isNotEmpty()) TextButton(onClick = model::clearPicked) { Text("Limpar") }
            }
            model.picked.forEach { Text("• ${it.name}") }
            model.pickNote?.let { Text(it, color = MaterialTheme.colorScheme.error, style = MaterialTheme.typography.labelMedium) }
            model.actionMessage?.let { Text(it, color = if (model.actionIsError) MaterialTheme.colorScheme.error else MaterialTheme.colorScheme.primary) }
            Button(onClick = { model.createTicket(text) }, enabled = !model.actionBusy, modifier = Modifier.fillMaxWidth()) {
                Text(if (model.actionBusy) "Enviando…" else "Abrir chamado")
            }
        }
    }
}
