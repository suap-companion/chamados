package br.edu.ifrn.chamados

import androidx.activity.compose.BackHandler
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
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
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.Icon
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.FilterChip
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.TopAppBar
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.ArrowDropDown
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateListOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
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

/**
 * As ações do chamado em menus suspensos: **Ações** (as da situação, cada uma pede confirmação), **Gestão**
 * (atribuir, escalar, reclassificar, tags, interessados) e **Mais** (o que é só seu: arquivar, adiar e
 * exportar). O resultado da última ação aparece logo abaixo dos menus.
 */
@OptIn(ExperimentalLayoutApi::class)
@Composable
fun ActionsSection(model: MainViewModel, view: TicketView) {
    var pending by remember(view.id) { mutableStateOf<DetailAction?>(null) }
    var managing by remember(view.id) { mutableStateOf<ManageAction?>(null) }
    val actions = DetailAction.available(view.statuses)
    Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
        FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
            MenuButton("Ações", enabled = actions.isNotEmpty() && !model.actionBusy) { close ->
                actions.forEach { action ->
                    DropdownMenuItem(text = { Text(action.label) }, onClick = { close(); model.dismissAction(); pending = action })
                }
            }
            MenuButton("Gestão", enabled = actions.isNotEmpty() && !model.actionBusy) { close ->
                ManageAction.entries.forEach { action ->
                    DropdownMenuItem(text = { Text(action.label) }, onClick = { close(); model.dismissAction(); managing = action })
                }
            }
            MoreMenu(model, view.id)
        }
        if (actions.isEmpty()) Text("Este chamado está encerrado: não há ações.", style = MaterialTheme.typography.bodyMedium)
        model.actionMessage?.let {
            Text(it, color = if (model.actionIsError) MaterialTheme.colorScheme.error else MaterialTheme.colorScheme.primary)
        }
        if (model.actionBusy) CircularProgressIndicator()
    }
    managing?.let { action ->
        ManageDialog(
            action = action,
            view = view,
            localTitle = model.titles[view.id],
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

/** Um botão que abre um menu suspenso; `items` recebe a função que fecha o menu. */
@Composable
private fun MenuButton(label: String, enabled: Boolean = true, items: @Composable (close: () -> Unit) -> Unit) {
    var open by remember { mutableStateOf(false) }
    Box {
        OutlinedButton(onClick = { open = true }, enabled = enabled) {
            Text(label)
            Icon(Icons.Filled.ArrowDropDown, contentDescription = null)
        }
        DropdownMenu(expanded = open, onDismissRequest = { open = false }) { items { open = false } }
    }
}

/** O que é só seu no chamado (o SUAP não tem): arquivar, adiar e exportar como ZIP. */
@Composable
private fun MoreMenu(model: MainViewModel, id: String) {
    val context = LocalContext.current
    val current = model.snoozes[id]?.takeIf { !it.due }
    val isArchived = id in model.archived
    var snoozing by remember(id) { mutableStateOf(false) }
    var exporting by remember(id) { mutableStateOf(false) }
    // Os avisos de "voltou" são notificações: pede a permissão na primeira vez que se adia algo.
    val permission = rememberLauncherForActivityResult(ActivityResultContracts.RequestPermission()) { }
    MenuButton("Mais") { close ->
        DropdownMenuItem(
            text = { Text(if (isArchived) "Desarquivar" else "Arquivar (só no app e na sincronização)") },
            onClick = { close(); model.setArchived(id, !isArchived) },
        )
        DropdownMenuItem(
            text = { Text(if (current != null) "Alterar a data do adiamento" else "Adiar… (volta sozinho depois)") },
            onClick = { close(); snoozing = true },
        )
        if (current != null) {
            DropdownMenuItem(text = { Text("Cancelar adiamento") }, onClick = { close(); model.unsnooze(id) })
        }
        DropdownMenuItem(
            text = { Text(if (model.exporting) "Gerando o ZIP…" else "Exportar como ZIP") },
            enabled = !model.exporting,
            onClick = { close(); exporting = true },
        )
    }
    if (snoozing) {
        SnoozeDialog(
            onPick = { until ->
                snoozing = false
                model.snooze(id, until)
                if (!Watcher.canNotify(context)) permission.launch(android.Manifest.permission.POST_NOTIFICATIONS)
            },
            onDismiss = { snoozing = false },
        )
    }
    if (exporting) {
        AlertDialog(
            onDismissRequest = { exporting = false },
            title = { Text("Exportar o chamado #$id") },
            text = { Text("O ZIP leva o texto (chamado.md), os dados (chamado.json) e, se quiser, os anexos. Depois você escolhe para onde enviá-lo.") },
            confirmButton = { TextButton(onClick = { exporting = false; model.exportTicket(id, true) }) { Text("Com anexos") } },
            dismissButton = {
                Row {
                    TextButton(onClick = { exporting = false; model.exportTicket(id, false) }) { Text("Sem anexos") }
                    TextButton(onClick = { exporting = false }) { Text("Cancelar") }
                }
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
                    action.draftKind()?.let { kind ->
                        AssistButton(kind, model.detail, model.detail?.let { model.titles[it.id] }, text) { text = it }
                    }
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
internal fun ManageDialog(
    action: ManageAction,
    view: TicketView?,
    localTitle: String?,
    onDismiss: () -> Unit,
    onConfirm: (List<String>) -> Unit,
) {
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
                    // A nota de escalar e de devolver é a primeira linha: a IA escreve a partir do chamado.
                    if (index == 0) {
                        action.draftKind()?.let { kind ->
                            AssistButton(kind, view, localTitle, values[0]) { values[0] = it }
                        }
                    }
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
        topBar = { GovBrHeader("Novo chamado", onBack = model::back) },
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
