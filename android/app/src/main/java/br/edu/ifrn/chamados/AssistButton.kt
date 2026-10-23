package br.edu.ifrn.chamados

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.unit.dp
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.Job
import kotlinx.coroutines.launch
import uniffi.chamados_mobile.DraftInput
import uniffi.chamados_mobile.DraftKind
import uniffi.chamados_mobile.DraftRefine
import uniffi.chamados_mobile.TicketView
import uniffi.chamados_mobile.assistClean
import uniffi.chamados_mobile.assistPrompt
import uniffi.chamados_mobile.assistTemplate

/** O texto que cada ação do detalhe escreve (`null` nas que não têm texto). */
fun DetailAction.draftKind(): DraftKind? = when (this) {
    DetailAction.Comment -> DraftKind.COMMENT
    DetailAction.Note -> DraftKind.INTERNAL_NOTE
    DetailAction.Resolve -> DraftKind.RESOLVE
    DetailAction.Suspend -> DraftKind.SUSPEND
    DetailAction.Cancel -> DraftKind.CANCEL
    DetailAction.Reopen -> DraftKind.REOPEN
    DetailAction.Close -> DraftKind.CLOSE
    else -> null
}

/** O texto que a gestão escreve na primeira linha (a nota de escalar e de devolver). */
fun ManageAction.draftKind(): DraftKind? = when (this) {
    ManageAction.Escalate -> DraftKind.ESCALATE
    ManageAction.Return -> DraftKind.RETURN
    else -> null
}

/**
 * "Sugerir com IA" num campo de texto: a IA do aparelho escreve um rascunho a partir do chamado aberto e
 * do que a pessoa já digitou no campo; depois dá para pedir "mais curto", "mais formal" ou "refazer". O
 * rascunho só vai ao campo (editável); nada é enviado ao SUAP sem a confirmação do diálogo. Sem IA no
 * aparelho, coloca um modelo com `[preencher]` para a pessoa completar.
 */
@OptIn(ExperimentalLayoutApi::class)
@Composable
fun AssistButton(
    kind: DraftKind,
    view: TicketView?,
    localTitle: String?,
    text: String,
    /** Só para testes: um provedor no lugar do escolhido pelas configurações. */
    provider: AnswerProvider? = null,
    onText: (String) -> Unit,
) {
    val scope = rememberCoroutineScope()
    val context = LocalContext.current
    var job by remember { mutableStateOf<Job?>(null) }
    var hasDraft by remember { mutableStateOf(false) }
    var message by remember { mutableStateOf<String?>(null) }
    var downloadable by remember { mutableStateOf(false) }
    var sources by remember { mutableStateOf<List<String>>(emptyList()) }
    var downloaded by remember { mutableStateOf<Long?>(null) }
    // Estado do Compose (e não `job.isActive`, que não recompõe a tela ao terminar nem ao cancelar).
    var busy by remember { mutableStateOf(false) }

    fun suggest(refine: DraftRefine?) {
        val ticket = view ?: return
        busy = true
        job = scope.launch {
            message = null
            try {
                val chosen = provider ?: Assistant.provider(context)
                val state = chosen.state()
                downloadable = state == AiState.Downloadable
                val said = text.trim().ifEmpty { null }
                // Documentos de apoio: só com IA pronta (o modelo pronto não os usa) e só os trechos mais relevantes.
                val references = if (state == AiState.Ready) Documents.references(context, ticket, said, localTitle) else emptyList()
                val input = DraftInput(
                    kind = kind, ticket = ticket, localTitle = localTitle,
                    hint = if (refine == null) said else null,
                    previous = if (refine != null) said else null,
                    refine = refine,
                    references = references,
                )
                sources = references.map { reference -> reference.source + (reference.page?.let { page -> ", p. $page" } ?: "") }
                if (state == AiState.Ready) {
                    val prompt = assistPrompt(input, null)
                    chosen.generate(prompt.text).collect { onText(assistClean(kind, it)) }
                    hasDraft = true
                    if (prompt.truncated) message = "O histórico do chamado era longo: usei só o mais recente."
                } else if (refine != null) {
                    message = "Os ajustes precisam da IA do aparelho, que não está pronta."
                } else {
                    onText(assistTemplate(input))
                    hasDraft = true
                    message = when (state) {
                        AiState.Downloadable -> "A IA do aparelho ainda não foi baixada. Coloquei um modelo para você completar."
                        AiState.Downloading -> "A IA do aparelho está sendo baixada. Coloquei um modelo para você completar."
                        else -> if (Assistant.enabled(context)) {
                            "Este aparelho não tem IA disponível. Coloquei um modelo para você completar. Dá para ligar a IA na nuvem em Configurações."
                        } else {
                            "O assistente de IA está desligado em Configurações. Coloquei um modelo para você completar."
                        }
                    }
                }
            } catch (cancelled: CancellationException) {
                throw cancelled
            } catch (error: Throwable) {
                message = error.assistMessage()
            } finally {
                busy = false
            }
        }
    }

    fun download() {
        busy = true
        job = scope.launch {
            message = null
            downloaded = 0
            try {
                Assistant.provider(context).download().collect { downloaded = it }
                downloadable = false
                message = "IA do aparelho pronta. Toque em Sugerir com IA."
            } catch (cancelled: CancellationException) {
                throw cancelled
            } catch (error: Throwable) {
                message = error.assistMessage()
            } finally {
                downloaded = null
                busy = false
            }
        }
    }

    Column(verticalArrangement = Arrangement.spacedBy(2.dp)) {
        FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            if (busy) {
                OutlinedButton(onClick = { job?.cancel() }) { Text("Parar") }
            } else {
                OutlinedButton(onClick = { suggest(null) }, enabled = view != null) { Text("Sugerir com IA") }
                if (hasDraft) {
                    TextButton(onClick = { suggest(DraftRefine.SHORTER) }) { Text("Mais curto") }
                    TextButton(onClick = { suggest(DraftRefine.MORE_FORMAL) }) { Text("Mais formal") }
                    TextButton(onClick = { suggest(DraftRefine.REDO) }) { Text("Refazer") }
                }
                if (downloadable) TextButton(onClick = { download() }) { Text("Baixar a IA do aparelho") }
            }
        }
        downloaded?.let { Text("Baixando a IA do aparelho… ${it / 1_000_000} MB", style = MaterialTheme.typography.labelMedium) }
        message?.let { Text(it, style = MaterialTheme.typography.labelMedium, color = MaterialTheme.colorScheme.onSurfaceVariant) }
        if (sources.isNotEmpty()) {
            Text(
                "Documentos de apoio considerados: " + sources.joinToString("; "),
                style = MaterialTheme.typography.labelMedium,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        }
        Text(
            "Revise antes de enviar: a IA pode errar e nada é enviado sem a sua confirmação.",
            style = MaterialTheme.typography.labelSmall,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
    }
}
