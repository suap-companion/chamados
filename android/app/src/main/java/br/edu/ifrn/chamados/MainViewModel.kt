package br.edu.ifrn.chamados

import android.app.Application
import android.net.Uri
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
import uniffi.chamados_mobile.ChamadosException
import uniffi.chamados_mobile.ListQuery
import uniffi.chamados_mobile.Queue
import uniffi.chamados_mobile.ShareJob
import uniffi.chamados_mobile.ShareKind
import uniffi.chamados_mobile.TicketSummary
import uniffi.chamados_mobile.TicketView

const val PRODUCTION_URL = "https://suap.ifrn.edu.br/"
const val LOCAL_URL = "http://10.0.2.2:8000/"

/** Em qual tela o app está. */
sealed interface Screen {
    data object Start : Screen
    data object Login : Screen
    data object Tickets : Screen
    data class Detail(val id: String) : Screen
    data object Settings : Screen
    data object NewTicket : Screen
}

/** O que se pode fazer num chamado, na tela de detalhe. */
enum class DetailAction(val label: String, val needsText: Boolean, val kind: ShareKind?, val takesFiles: Boolean) {
    Assume("Assumir", false, null, false),
    Start("Iniciar atendimento", false, null, false),
    Comment("Comentar", true, ShareKind.COMMENT, true),
    Note("Nota interna", true, ShareKind.INTERNAL_NOTE, true),
    Attach("Anexar arquivo", false, ShareKind.ATTACH, true),
    Resolve("Resolver", true, ShareKind.RESOLVE, true),
    Suspend("Suspender", true, ShareKind.SUSPEND, true),
    Cancel("Cancelar chamado", true, ShareKind.CANCEL, true),
    Reopen("Reabrir", true, ShareKind.REOPEN, true),
    Close("Fechar e avaliar", false, null, false),
    ;

    companion object {
        /** As ações que fazem sentido para a situação do chamado (o SUAP ainda decide pelas permissões). */
        fun available(statuses: List<String>): List<DetailAction> {
            val text = statuses.joinToString(" ").lowercase()
            return when {
                "fechado" in text || "cancelado" in text -> emptyList()
                "resolvido" in text -> listOf(Reopen, Close, Comment, Note, Attach)
                else -> listOf(Assume, Start, Comment, Note, Attach, Resolve, Suspend, Cancel)
            }
        }
    }
}

/** Um anexo baixado, pronto para abrir. */
data class DownloadedFile(val path: String)

/** O texto de um erro da biblioteca Rust, como a pessoa deve ler. */
fun Throwable.readable(): String =
    (this as? ChamadosException.Failed)?.detail ?: (message ?: toString())

class MainViewModel(application: Application) : AndroidViewModel(application) {
    private var core: Chamados? = null

    var screen by mutableStateOf<Screen>(Screen.Start)
        private set
    var baseUrl by mutableStateOf(CoreAccess.baseUrl(application))
        private set

    // Lista
    var queue by mutableStateOf(Queue.MINE)
        private set
    var searchText by mutableStateOf("")
    var tickets by mutableStateOf<List<TicketSummary>>(emptyList())
        private set
    var listLoading by mutableStateOf(false)
        private set
    var listError by mutableStateOf<String?>(null)
        private set

    // Detalhe
    var detail by mutableStateOf<TicketView?>(null)
        private set
    var detailLoading by mutableStateOf(false)
        private set
    var detailError by mutableStateOf<String?>(null)
        private set
    var downloading by mutableStateOf<UInt?>(null)
        private set
    var downloaded by mutableStateOf<DownloadedFile?>(null)

    // Ações e novo chamado
    var actionBusy by mutableStateOf(false)
        private set
    var actionMessage by mutableStateOf<String?>(null)
    var actionIsError by mutableStateOf(false)
        private set
    var picked by mutableStateOf<List<ReceivedFile>>(emptyList())
        private set
    var pickNote by mutableStateOf<String?>(null)
        private set
    var serviceId by mutableStateOf(CoreAccess.serviceId(application))
    var interested by mutableStateOf(CoreAccess.interested(application))

    init {
        start()
    }

    private fun dataDirFor(url: String): File = CoreAccess.dataDir(getApplication(), url)

    private fun open(): Chamados {
        core?.let { return it }
        val dir = dataDirFor(baseUrl).apply { mkdirs() }
        return Chamados(dir.absolutePath, baseUrl).also { core = it }
    }

    /** O objeto da biblioteca para a tela de login importar os cookies. */
    fun session(): Chamados = open()

    fun start() {
        screen = Screen.Start
        viewModelScope.launch {
            try {
                if (open().isAuthenticated()) openTickets() else screen = Screen.Login
            } catch (error: Throwable) {
                // Sem rede ou endereço errado: a pessoa corrige nas configurações.
                listError = error.readable()
                screen = Screen.Settings
            }
        }
    }

    fun loggedIn() {
        openTickets()
    }

    fun openTickets() {
        screen = Screen.Tickets
        refresh()
    }

    fun chooseQueue(next: Queue) {
        queue = next
        if (next != Queue.SUPPORT) searchText = ""
        refresh()
    }

    fun refresh() {
        viewModelScope.launch {
            listLoading = true
            listError = null
            try {
                val text = searchText.trim().ifEmpty { null }
                val query = ListQuery(text, emptyList(), false, null, null, null)
                tickets = open().list(queue, query)
            } catch (error: Throwable) {
                if (!stillLoggedIn()) screen = Screen.Login else listError = error.readable()
            } finally {
                listLoading = false
            }
        }
    }

    fun openDetail(id: String) {
        screen = Screen.Detail(id)
        detail = null
        actionMessage = null
        downloaded = null
        viewModelScope.launch {
            detailLoading = true
            detailError = null
            try {
                detail = open().show(id)
            } catch (error: Throwable) {
                if (!stillLoggedIn()) screen = Screen.Login else detailError = error.readable()
            } finally {
                detailLoading = false
            }
        }
    }

    fun download(id: String, number: UInt) {
        viewModelScope.launch {
            downloading = number
            detailError = null
            try {
                val folder = File(getApplication<Application>().cacheDir, "anexos").apply { mkdirs() }
                // Um download por vez e sem acumular: a pasta é só de passagem para abrir o arquivo.
                folder.listFiles()?.forEach { it.delete() }
                val path = open().downloadAttachment(id, number, folder.absolutePath)
                downloaded = DownloadedFile(path)
            } catch (error: Throwable) {
                detailError = error.readable()
            } finally {
                downloading = null
            }
        }
    }

    /** A sessão ainda vale? Se não der para perguntar, supõe que sim e deixa o erro aparecer. */
    private suspend fun stillLoggedIn(): Boolean =
        try {
            open().isAuthenticated()
        } catch (_: Throwable) {
            true
        }

    /** Atualiza o detalhe depois de uma ação, sem sair da tela. */
    private suspend fun reloadDetail(id: String) {
        runCatching { open().show(id) }.onSuccess { detail = it }
    }

    private val pickFolder get() = File(getApplication<Application>().cacheDir, "envio")

    /** Copia o que a pessoa escolheu no seletor de arquivos e separa o que o SUAP aceita. */
    fun onPicked(uris: List<Uri>, limit: UInt?) {
        viewModelScope.launch {
            val (accepted, refused) = withContext(Dispatchers.IO) {
                pickFolder.deleteRecursively()
                pickFolder.mkdirs()
                val context = getApplication<Application>()
                val copied = uris.mapIndexedNotNull { index, uri -> FileCopy.copy(context, uri, pickFolder, index) }
                FileCopy.classify(copied, limit)
            }
            picked = accepted
            pickNote = refused.takeIf { it.isNotEmpty() }
                ?.joinToString("\n") { "${it.name}: ${it.reason}" }
        }
    }

    fun clearPicked() {
        picked = emptyList()
        pickNote = null
        pickFolder.deleteRecursively()
    }

    fun dismissAction() {
        actionMessage = null
        clearPicked()
    }

    private fun describe(steps: List<uniffi.chamados_mobile.ShareStepResult>): String =
        steps.joinToString("\n") { step ->
            val mark = if (step.error == null) "✓" else "✗"
            val reason = step.error?.let { ": $it" } ?: ""
            "$mark ${step.label}$reason"
        }

    /** Executa uma ação do detalhe e mostra o resultado; só atualiza o chamado se deu certo. */
    fun runAction(id: String, action: DetailAction, text: String?, rating: Int?) {
        viewModelScope.launch {
            actionBusy = true
            actionMessage = null
            try {
                val core = open()
                val clean = text?.trim()?.ifEmpty { null }
                var failure: String? = null
                when (action) {
                    DetailAction.Assume -> core.assume(id)
                    DetailAction.Start -> core.startService(id)
                    DetailAction.Close -> core.closeTicket(id, rating?.toUByte(), clean)
                    else -> {
                        val job = ShareJob(
                            kind = action.kind!!, serviceId = null, campus = null, center = null, interested = null,
                            ticket = id, text = clean, title = null,
                            filePaths = picked.map { it.path }, resumeFrom = 0u,
                        )
                        val outcome = core.applyShare(job)
                        if (!outcome.done) failure = describe(outcome.steps)
                    }
                }
                actionIsError = failure != null
                actionMessage = failure ?: "${action.label}: feito."
                if (failure == null) reloadDetail(id)
            } catch (error: Throwable) {
                actionIsError = true
                actionMessage = error.readable()
            } finally {
                clearPicked()
                actionBusy = false
            }
        }
    }

    /** Executa uma ação de gestão (atribuir, escalar, reclassificar, tags, interessados). */
    fun runManage(id: String, action: ManageAction, values: List<String>) {
        viewModelScope.launch {
            actionBusy = true
            actionMessage = null
            try {
                val core = open()
                fun at(index: Int): String? = values.getOrNull(index)?.trim()?.ifEmpty { null }
                fun list(index: Int): List<String> =
                    values.getOrNull(index).orEmpty().split(',', ';', '\n').map { it.trim() }.filter { it.isNotEmpty() }
                val result = when (action) {
                    ManageAction.Assign -> "Atribuído a ${core.assign(id, at(0).orEmpty())}."
                    ManageAction.Escalate -> {
                        core.escalate(id, at(0).orEmpty(), at(1))
                        "Escalado para o grupo acima."
                    }
                    ManageAction.Return -> {
                        core.returnTicket(id, at(0).orEmpty(), at(1))
                        "Devolvido ao grupo abaixo."
                    }
                    ManageAction.Reclassify -> {
                        core.reclassify(id, at(0), at(1), at(2), at(3).orEmpty())
                        "Chamado reclassificado."
                    }
                    ManageAction.AddTags -> "Tags adicionadas: ${core.addTags(id, list(0)).joinToString()}."
                    ManageAction.RemoveTags -> "Tags removidas: ${core.removeTags(id, list(0)).joinToString()}."
                    ManageAction.AddInterested -> "Incluídos: ${core.addInterested(id, list(0)).joinToString()}."
                    ManageAction.RemoveInterested -> "Removidos: ${core.removeInterested(id, list(0)).joinToString()}."
                }
                actionIsError = false
                actionMessage = result
                reloadDetail(id)
            } catch (error: Throwable) {
                actionIsError = true
                actionMessage = error.readable()
            } finally {
                actionBusy = false
            }
        }
    }

    fun openNewTicket() {
        actionMessage = null
        clearPicked()
        screen = Screen.NewTicket
    }

    /** Abre um chamado novo (o serviço é o número dele no SUAP) e vai para o detalhe. */
    fun createTicket(text: String) {
        val service = serviceId.trim().toLongOrNull()
        if (service == null || text.isBlank()) {
            actionIsError = true
            actionMessage = "Informe o número do serviço e a descrição."
            return
        }
        viewModelScope.launch {
            actionBusy = true
            actionMessage = null
            try {
                val job = ShareJob(
                    kind = ShareKind.CREATE, serviceId = service.toULong(), campus = null, center = null,
                    interested = interested.trim().ifEmpty { null },
                    ticket = null, text = text.trim(), title = null,
                    filePaths = picked.map { it.path }, resumeFrom = 0u,
                )
                val outcome = open().applyShare(job)
                val created = outcome.ticket
                if (outcome.done && created != null) {
                    CoreAccess.saveServiceId(getApplication(), service.toString())
                    CoreAccess.saveInterested(getApplication(), interested.trim())
                    clearPicked()
                    openDetail(created)
                } else {
                    actionIsError = true
                    actionMessage = describe(outcome.steps)
                }
            } catch (error: Throwable) {
                actionIsError = true
                actionMessage = error.readable()
            } finally {
                actionBusy = false
            }
        }
    }

    fun logout() {
        viewModelScope.launch {
            runCatching { open().logout() }
            tickets = emptyList()
            screen = Screen.Login
        }
    }

    fun back() {
        when (screen) {
            is Screen.Detail -> screen = Screen.Tickets
            Screen.NewTicket -> screen = Screen.Tickets
            Screen.Settings -> start()
            else -> Unit
        }
    }

    fun openSettings() {
        screen = Screen.Settings
    }

    fun changeBaseUrl(url: String) {
        val clean = url.trim().let { if (it.endsWith("/")) it else "$it/" }
        CoreAccess.saveBaseUrl(getApplication(), clean)
        baseUrl = clean
        core = null
        listError = null
        tickets = emptyList()
        start()
    }
}
