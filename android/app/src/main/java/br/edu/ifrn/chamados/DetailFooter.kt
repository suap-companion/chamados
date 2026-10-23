package br.edu.ifrn.chamados

import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Button
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.unit.dp
import uniffi.chamados_mobile.TicketView

/** Um passo que o rodapé do detalhe oferece: o botão principal ou a alternativa. */
enum class FooterStep(val label: String, val action: DetailAction?) {
    /** Assume e, em seguida, inicia o atendimento (duas chamadas ao SUAP). */
    AssumeAndStart("Assumir e iniciar", null),
    Assume("Só assumir", DetailAction.Assume),
    Resume("Retomar atendimento", DetailAction.Start),
    Resolve("Resolver", DetailAction.Resolve),
    Suspend("Suspender", DetailAction.Suspend),
    CloseTicket("Fechar e avaliar", DetailAction.Close),
    Reopen("Reabrir", DetailAction.Reopen),
}

/** O que o rodapé mostra para uma situação: o principal, uma alternativa e se dá para cancelar. */
data class FooterPlan(val primary: FooterStep?, val secondary: FooterStep?, val canCancel: Boolean) {
    /** Fechado ou cancelado: sem rodapé. */
    val isEmpty: Boolean get() = primary == null && !canCancel
}

/** A ação principal conforme a situação do chamado (o SUAP ainda decide pelas permissões). */
fun footerPlan(statuses: List<String>): FooterPlan {
    val text = statuses.joinToString(" ").lowercase()
    return when {
        "fechado" in text || "cancelado" in text -> FooterPlan(null, null, canCancel = false)
        "resolvido" in text -> FooterPlan(FooterStep.CloseTicket, FooterStep.Reopen, canCancel = false)
        "suspens" in text -> FooterPlan(FooterStep.Resume, null, canCancel = true)
        "atendimento" in text -> FooterPlan(FooterStep.Resolve, FooterStep.Suspend, canCancel = true)
        else -> FooterPlan(FooterStep.AssumeAndStart, FooterStep.Assume, canCancel = true)
    }
}

/** A mensagem de "Assumir e iniciar": se o segundo passo falha, o chamado já ficou assumido e a mensagem diz isso. */
fun assumeAndStartMessage(startFailure: String?): String =
    if (startFailure == null) "Assumir e iniciar: feito."
    else "O chamado foi assumido, mas não foi possível iniciar o atendimento: $startFailure"

/** O rodapé fixo do detalhe, sem estado: o que mostrar e o que fazer ao tocar. */
@Composable
fun DetailFooterContent(
    plan: FooterPlan,
    busy: Boolean,
    message: String?,
    messageIsError: Boolean,
    onStep: (FooterStep) -> Unit,
    onSnooze: () -> Unit,
    onCancel: () -> Unit,
) {
    if (plan.isEmpty) return
    Surface(tonalElevation = 3.dp, modifier = Modifier.fillMaxWidth()) {
        Column(Modifier.padding(horizontal = 16.dp, vertical = 12.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
            message?.let {
                Text(it, style = MaterialTheme.typography.bodySmall, color = if (messageIsError) MaterialTheme.colorScheme.error else MaterialTheme.colorScheme.primary)
            }
            plan.primary?.let { step ->
                Button(onClick = { onStep(step) }, enabled = !busy, modifier = Modifier.fillMaxWidth().heightIn(min = 48.dp)) { Text(step.label) }
                Row(horizontalArrangement = Arrangement.spacedBy(8.dp), modifier = Modifier.fillMaxWidth()) {
                    plan.secondary?.let { second ->
                        OutlinedButton(onClick = { onStep(second) }, enabled = !busy, modifier = Modifier.weight(1f).heightIn(min = 48.dp)) { Text(second.label) }
                    }
                    OutlinedButton(onClick = onSnooze, modifier = Modifier.weight(1f).heightIn(min = 48.dp)) { Text("Adiar") }
                }
            }
            if (plan.canCancel) {
                TextButton(onClick = onCancel, enabled = !busy, modifier = Modifier.fillMaxWidth().heightIn(min = 48.dp)) {
                    Text("Cancelar chamado", color = MaterialTheme.colorScheme.error)
                }
            }
        }
    }
}

/** O rodapé do detalhe ligado ao app: cada passo abre a confirmação de sempre (com texto, quando a ação pede). */
@Composable
fun DetailFooterBar(model: MainViewModel, view: TicketView) {
    val context = LocalContext.current
    val plan = remember(view.statuses) { footerPlan(view.statuses) }
    var pending by remember(view.id) { mutableStateOf<DetailAction?>(null) }
    var confirmBoth by remember(view.id) { mutableStateOf(false) }
    var snoozing by remember(view.id) { mutableStateOf(false) }
    // Os avisos de "voltou" são notificações: pede a permissão na primeira vez que se adia algo.
    val permission = rememberLauncherForActivityResult(ActivityResultContracts.RequestPermission()) { }

    DetailFooterContent(
        plan = plan,
        busy = model.actionBusy,
        message = model.actionMessage,
        messageIsError = model.actionIsError,
        onStep = { step ->
            model.dismissAction()
            if (step.action == null) confirmBoth = true else pending = step.action
        },
        onSnooze = { snoozing = true },
        onCancel = { model.dismissAction(); pending = DetailAction.Cancel },
    )

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
    if (confirmBoth) {
        AlertDialog(
            onDismissRequest = { confirmBoth = false },
            title = { Text("Assumir e iniciar?") },
            text = { Text("O chamado #${view.id} será atribuído a você e o atendimento começa em seguida.") },
            confirmButton = { TextButton(onClick = { confirmBoth = false; model.runAssumeAndStart(view.id) }) { Text("Assumir e iniciar") } },
            dismissButton = { TextButton(onClick = { confirmBoth = false }) { Text("Voltar") } },
        )
    }
    if (snoozing) {
        SnoozeDialog(
            onPick = { until ->
                snoozing = false
                model.snooze(view.id, until)
                if (!Watcher.canNotify(context)) permission.launch(android.Manifest.permission.POST_NOTIFICATIONS)
            },
            onDismiss = { snoozing = false },
        )
    }
}
