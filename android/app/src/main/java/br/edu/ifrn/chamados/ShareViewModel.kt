package br.edu.ifrn.chamados

import android.app.Application
import android.content.Intent
import android.net.Uri
import android.provider.OpenableColumns
import android.webkit.MimeTypeMap
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.lifecycle.AndroidViewModel
import androidx.lifecycle.viewModelScope
import java.io.File
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import uniffi.chamados_mobile.Chamados
import uniffi.chamados_mobile.ListQuery
import uniffi.chamados_mobile.Queue
import uniffi.chamados_mobile.RejectedFile
import uniffi.chamados_mobile.ShareJob
import uniffi.chamados_mobile.ShareKind
import uniffi.chamados_mobile.ShareOutcome
import uniffi.chamados_mobile.SharedFileInfo
import uniffi.chamados_mobile.TicketSummary
import uniffi.chamados_mobile.classifyShared

/** Um arquivo recebido, já copiado para a pasta do app. */
data class ReceivedFile(val name: String, val path: String, val size: Long)

/** Em que passo da conversa "O que fazer com isto?" a pessoa está. */
enum class SharePhase { Loading, NeedsLogin, Choosing, Sending, Finished }

/** As ações possíveis, na ordem em que aparecem, com o rótulo e se pedem um chamado de destino. */
enum class ShareChoice(val kind: ShareKind, val label: String, val needsTicket: Boolean, val needsText: Boolean) {
    Create(ShareKind.CREATE, "Criar chamado", false, true),
    Attach(ShareKind.ATTACH, "Anexar a um chamado", true, false),
    Comment(ShareKind.COMMENT, "Comentar", true, true),
    Note(ShareKind.INTERNAL_NOTE, "Nota interna", true, true),
    Resolve(ShareKind.RESOLVE, "Resolver (finalizar)", true, true),
    Cancel(ShareKind.CANCEL, "Cancelar chamado", true, true),
    Reopen(ShareKind.REOPEN, "Reabrir", true, true),
    Suspend(ShareKind.SUSPEND, "Suspender", true, true),
}

class ShareViewModel(application: Application) : AndroidViewModel(application) {
    private val folder = File(application.filesDir, "shared")
    private var core: Chamados? = null

    var phase by mutableStateOf(SharePhase.Loading)
        private set
    var error by mutableStateOf<String?>(null)
        private set

    // O que chegou
    var files by mutableStateOf<List<ReceivedFile>>(emptyList())
        private set
    var rejected by mutableStateOf<List<RejectedFile>>(emptyList())
        private set
    var text by mutableStateOf("")
    var title by mutableStateOf<String?>(null)
        private set

    // O que a pessoa decide
    var choice by mutableStateOf<ShareChoice?>(null)
    var ticket by mutableStateOf("")
    var serviceId by mutableStateOf("")
    var confirming by mutableStateOf(false)

    var tickets by mutableStateOf<List<TicketSummary>>(emptyList())
        private set
    var outcome by mutableStateOf<ShareOutcome?>(null)
        private set

    private var started = false

    /** Recebe o que chegou por "Compartilhar": copia os arquivos e confere a sessão. */
    fun receive(intent: Intent) {
        if (started) return
        started = true
        viewModelScope.launch {
            try {
                val received = withContext(Dispatchers.IO) { copyAll(intent) }
                text = (intent.getCharSequenceExtra(Intent.EXTRA_TEXT) ?: "").toString()
                title = intent.getStringExtra(Intent.EXTRA_SUBJECT)
                files = received
                classify()
                checkSession()
            } catch (failure: Throwable) {
                error = failure.readable()
                phase = SharePhase.Choosing
            }
        }
    }

    private fun classify() {
        val info = files.map { SharedFileInfo(it.name, it.size.toULong()) }
        val result = classifyShared(info, null)
        val accepted = result.accepted.map { it.toInt() }.toSet()
        rejected = result.rejected
        files = files.filterIndexed { index, _ -> index in accepted }
    }

    /** Sem sessão válida não há o que enviar: a pessoa entra no app e volta aqui. */
    fun checkSession() {
        phase = SharePhase.Loading
        viewModelScope.launch {
            try {
                val chamados = CoreAccess.open(getApplication()).also { core = it }
                if (!chamados.isAuthenticated()) {
                    phase = SharePhase.NeedsLogin
                    return@launch
                }
                phase = SharePhase.Choosing
                loadTickets(chamados)
            } catch (failure: Throwable) {
                error = failure.readable()
                phase = SharePhase.Choosing
            }
        }
    }

    /** Os chamados que a pessoa acompanha (e, se for atendente, os da fila) para escolher o destino. */
    private suspend fun loadTickets(chamados: Chamados) {
        val query = ListQuery(null, emptyList(), false, null, null, null)
        val mine = runCatching { chamados.list(Queue.MINE, query) }.getOrDefault(emptyList())
        val support = runCatching { chamados.list(Queue.SUPPORT, query) }.getOrDefault(emptyList())
        tickets = (mine + support).distinctBy { it.id }
    }

    /** Por que a ação não pode ser escolhida agora; `null` quando pode. */
    fun unavailable(option: ShareChoice): String? {
        val hasText = text.isNotBlank()
        return when {
            option == ShareChoice.Attach && !hasText && files.isEmpty() -> "não há arquivo aceito nem texto"
            option.needsText && !hasText -> "falta o texto (descrição, mensagem ou motivo)"
            option == ShareChoice.Create && files.size > 3 -> "um chamado novo aceita até 3 anexos"
            else -> null
        }
    }

    /** O que falta para poder enviar; `null` quando está pronto. */
    fun missing(): String? {
        val option = choice ?: return "Escolha o que fazer."
        unavailable(option)?.let { return it.replaceFirstChar { c -> c.uppercase() } + "." }
        if (option.needsTicket && ticket.isBlank()) return "Informe o chamado de destino."
        if (option == ShareChoice.Create && serviceId.trim().toLongOrNull() == null) return "Informe o número do serviço."
        return null
    }

    /** Mudou a ação ou o destino: o que já foi feito numa tentativa anterior não vale mais. */
    fun pick(option: ShareChoice) {
        choice = option
        outcome = null
    }

    fun pickTicket(id: String) {
        ticket = id
        outcome = null
    }

    fun send() {
        val option = choice ?: return
        confirming = false
        phase = SharePhase.Sending
        viewModelScope.launch {
            try {
                val job = ShareJob(
                    kind = option.kind,
                    serviceId = serviceId.trim().toLongOrNull()?.toULong().takeIf { option == ShareChoice.Create },
                    campus = null,
                    center = null,
                    ticket = ticket.trim().takeIf { option.needsTicket },
                    text = text.trim().ifEmpty { null },
                    title = title,
                    filePaths = files.map { it.path },
                    resumeFrom = (outcome?.completed ?: 0u),
                )
                val result = (core ?: CoreAccess.open(getApplication())).applyShare(job)
                outcome = result
                error = null
                if (result.done) cleanup()
            } catch (failure: Throwable) {
                // Pedido recusado antes de enviar: nada foi mandado ao SUAP.
                error = failure.readable()
            }
            phase = if (outcome?.done == true) SharePhase.Finished else SharePhase.Choosing
        }
    }

    /** Apaga as cópias temporárias dos arquivos recebidos. */
    fun cleanup() {
        folder.deleteRecursively()
    }

    override fun onCleared() {
        cleanup()
    }

    private fun copyAll(intent: Intent): List<ReceivedFile> {
        val uris = mutableListOf<Uri>()
        @Suppress("DEPRECATION")
        if (intent.action == Intent.ACTION_SEND_MULTIPLE) {
            intent.getParcelableArrayListExtra<Uri>(Intent.EXTRA_STREAM)?.let { uris.addAll(it) }
        } else {
            (intent.getParcelableExtra<Uri>(Intent.EXTRA_STREAM))?.let { uris.add(it) }
        }
        if (uris.isEmpty()) {
            // Quem compartilha pode mandar só o ClipData ou o data do intent.
            val clip = intent.clipData
            if (clip != null) (0 until clip.itemCount).mapNotNull { clip.getItemAt(it).uri }.let { uris.addAll(it) }
            intent.data?.takeIf { uris.isEmpty() }?.let { uris.add(it) }
        }
        folder.deleteRecursively()
        folder.mkdirs()
        return uris.mapIndexedNotNull { index, uri -> copyOne(uri, index) }
    }

    /** Copia um `content://` para a pasta do app (o app de origem pode revogar o acesso depois). */
    private fun copyOne(uri: Uri, index: Int): ReceivedFile? {
        val resolver = getApplication<Application>().contentResolver
        var name = uri.lastPathSegment?.substringAfterLast('/') ?: "arquivo-$index"
        resolver.query(uri, arrayOf(OpenableColumns.DISPLAY_NAME), null, null, null)?.use { cursor ->
            if (cursor.moveToFirst() && !cursor.isNull(0)) name = cursor.getString(0)
        }
        name = name.replace(Regex("[\\\\/:*?\"<>|]"), "_")
        if (!name.contains('.')) {
            val extension = resolver.getType(uri)?.let { MimeTypeMap.getSingleton().getExtensionFromMimeType(it) }
            if (extension != null) name = "$name.$extension"
        }
        val target = File(File(folder, index.toString()).apply { mkdirs() }, name)
        val input = resolver.openInputStream(uri) ?: return null
        input.use { source -> target.outputStream().use { source.copyTo(it) } }
        return ReceivedFile(name, target.absolutePath, target.length())
    }
}
