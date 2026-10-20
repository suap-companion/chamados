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
import uniffi.chamados_mobile.ListView
import uniffi.chamados_mobile.Queue
import uniffi.chamados_mobile.ShareJob
import uniffi.chamados_mobile.ShareKind
import uniffi.chamados_mobile.ListCounts
import uniffi.chamados_mobile.SnoozeItem
import uniffi.chamados_mobile.UserInfo
import uniffi.chamados_mobile.SyncSetup
import uniffi.chamados_mobile.SyncState
import uniffi.chamados_mobile.TicketSummary
import uniffi.chamados_mobile.TicketView

const val PRODUCTION_URL = "https://suap.ifrn.edu.br/"
const val LOCAL_URL = "http://10.0.2.2:8000/"

/** O que a lista mostra: as de sempre, os arquivados ou os adiados (marcas só nossas, sincronizadas). */
/** Quantas páginas seguidas sem nada novo a rolagem tenta antes de dar a lista por encerrada. */
private const val MAX_EMPTY_PAGES = 5

enum class ListMode { Normal, Archived, Snoozed }

/** As ações que valem para vários chamados de uma vez (seleção em lote). */
enum class BatchAction(val label: String) {
    Archive("Arquivar"),
    Unarchive("Desarquivar"),
    Snooze("Adiar"),
    Comment("Comentar"),
    Note("Nota interna"),
    AddTags("Adicionar tags"),
}

/** Em qual tela o app está. */
sealed interface Screen {
    data object Start : Screen
    data object Login : Screen
    data object Tickets : Screen
    data class Detail(val id: String) : Screen
    data object Settings : Screen
    data object NewTicket : Screen
    data object Sync : Screen
    data object Documents : Screen
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

    // Perfis (ambientes) do app e a pessoa logada no perfil em uso
    var profiles by mutableStateOf(Profiles.list(application))
        private set
    var activeProfile by mutableStateOf(Profiles.active(application))
        private set
    var user by mutableStateOf<UserInfo?>(null)
        private set
    var profileError by mutableStateOf<String?>(null)

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
    var exported by mutableStateOf<DownloadedFile?>(null)
    var exporting by mutableStateOf(false)
        private set

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

    // Títulos locais (número do chamado → título) e chamados arquivados (marca só nossa, sincronizada)
    var titles by mutableStateOf<Map<String, String>>(emptyMap())
        private set
    var archived by mutableStateOf<Set<String>>(emptySet())
        private set
    var mode by mutableStateOf(ListMode.Normal)
        private set
    /** Os adiados (número do chamado → quando volta, e se já voltou sem ninguém ver). */
    var snoozes by mutableStateOf<Map<String, SnoozeItem>>(emptyMap())
        private set
    /** Quantos chamados a aba tem em cada situação (os selos dos filtros); `null` até a primeira contagem. */
    var counts by mutableStateOf<ListCounts?>(null)
        private set
    /** A ordem da linha do tempo, lembrada entre as aberturas do app: `true` = do mais novo ao mais antigo. */
    var newestFirst by mutableStateOf(CoreAccess.newestFirst(application))
        private set

    fun toggleOrder() {
        newestFirst = !newestFirst
        CoreAccess.saveNewestFirst(getApplication(), newestFirst)
    }

    /** Os arquivos das fotos de quem pediu (caminho da foto no SUAP → arquivo no aparelho). */
    var photoFiles by mutableStateOf<Map<String, String>>(emptyMap())
        private set
    /** Os chamados escolhidos para uma ação em lote (toque longo na lista); vazio = sem seleção. */
    var selection by mutableStateOf<Set<String>>(emptySet())
        private set
    var batchBusy by mutableStateOf(false)
        private set
    /** O resumo da última ação em lote (o que deu certo e o que falhou). */
    var batchMessage by mutableStateOf<String?>(null)
    var batchIsError by mutableStateOf(false)
        private set
    /** Está carregando a página seguinte da lista (rolagem até o fim). */
    var loadingMore by mutableStateOf(false)
        private set
    /** A lista já mostrou tudo o que o SUAP tem para esta aba e situação. */
    var endReached by mutableStateOf(false)
        private set
    private var nextPage = 2u
    private var listGeneration = 0
    private var listStale = false

    // Sincronização
    var syncState by mutableStateOf<SyncState?>(null)
        private set
    var syncBusy by mutableStateOf(false)
        private set
    var syncMessage by mutableStateOf<String?>(null)
    var syncIsError by mutableStateOf(false)
        private set
    var shownKey by mutableStateOf<String?>(null)

    init {
        start()
    }

    private fun open(): Chamados {
        core?.let { return it }
        val dir = CoreAccess.dataDir(getApplication(), activeProfile).apply { mkdirs() }
        return Chamados(dir.absolutePath, activeProfile.url).also { core = it }
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
        loadTitles()
        loadUser()
    }

    /** A pessoa logada: mostra a que está guardada e atualiza com o que o SUAP mostra agora (nome, matrícula, foto). */
    private fun loadUser() {
        viewModelScope.launch {
            runCatching { open().cachedUser() }.onSuccess { user = it }
            runCatching { open().refreshUser() }.onSuccess { found -> if (found != null) user = found }
        }
    }

    /** Troca o perfil em uso: o app volta a abrir a sessão e os dados do outro perfil. */
    fun switchProfile(name: String) {
        Profiles.activate(getApplication(), name)
        reloadProfile()
    }

    /** Cadastra um perfil novo (nome e endereço do SUAP) e passa a usá-lo; devolve o erro, se houver. */
    fun addProfile(name: String, url: String): Boolean {
        val error = Profiles.add(getApplication(), name, url)
        profileError = error
        if (error == null) reloadProfile()
        return error == null
    }

    private fun reloadProfile() {
        activeProfile = Profiles.active(getApplication())
        profiles = Profiles.list(getApplication())
        baseUrl = activeProfile.url
        core = null
        user = null
        tickets = emptyList()
        titles = emptyMap()
        archived = emptySet()
        counts = null
        selection = emptySet()
        photoFiles = emptyMap()
        snoozes = emptyMap()
        listError = null
        mode = ListMode.Normal
        searchText = ""
        start()
    }

    /** Lê os títulos locais do disco (o app e a sincronização os mudam). */
    fun loadTitles() {
        viewModelScope.launch {
            runCatching { open().titles() }.onSuccess { found -> titles = found.associate { it.id to it.title } }
            runCatching { open().archivedIds() }.onSuccess { found -> archived = found.toSet() }
            runCatching { open().snoozed() }.onSuccess { found ->
                snoozes = found.associateBy { it.id }
                // Enquanto houver adiados, uma verificação local (sem rede) avisa quando um volta.
                Watcher.scheduleSnoozes(getApplication(), found.isNotEmpty())
            }
        }
    }

    /** Adia o chamado até `until` (ms desde 1970): some das listas e volta ao topo nessa hora. */
    fun snooze(id: String, until: ULong) {
        viewModelScope.launch {
            try {
                open().snooze(id, until)
                actionIsError = false
                actionMessage = "Adiado até ${formatMoment(until)}."
                listStale = true
                loadTitles()
            } catch (error: Throwable) {
                actionIsError = true
                actionMessage = error.readable()
            }
        }
    }

    /** Cancela o adiamento: o chamado volta às listas já. */
    fun unsnooze(id: String) {
        viewModelScope.launch {
            try {
                open().unsnooze(id)
                actionIsError = false
                actionMessage = "Adiamento cancelado."
                listStale = true
                loadTitles()
            } catch (error: Throwable) {
                actionIsError = true
                actionMessage = error.readable()
            }
        }
    }

    /** Gera o ZIP do chamado (texto, dados e, se pedido, os anexos) para entregar por "Compartilhar". */
    fun exportTicket(id: String, attachments: Boolean) {
        viewModelScope.launch {
            exporting = true
            actionMessage = null
            try {
                val folder = File(getApplication<Application>().cacheDir, "exportar").apply { mkdirs() }
                // Um ZIP por vez e sem acumular: a pasta é só de passagem.
                folder.listFiles()?.forEach { it.delete() }
                exported = DownloadedFile(open().exportTicket(id, attachments, folder.absolutePath))
            } catch (error: Throwable) {
                actionIsError = true
                actionMessage = error.readable()
            } finally {
                exporting = false
            }
        }
    }

    /** Arquiva (ou desarquiva) o chamado: some das listas, sem mudar nada no SUAP. */
    fun setArchived(id: String, value: Boolean) {
        viewModelScope.launch {
            try {
                open().setArchived(id, value)
                actionIsError = false
                actionMessage = if (value) "Chamado arquivado." else "Chamado desarquivado."
                listStale = true
                loadTitles()
            } catch (error: Throwable) {
                actionIsError = true
                actionMessage = error.readable()
            }
        }
    }

    /** Dá (ou, com texto vazio, tira) o título local do chamado. */
    fun saveTitle(id: String, text: String) {
        viewModelScope.launch {
            try {
                if (text.isBlank()) open().removeTitle(id) else open().setTitle(id, text)
                actionIsError = false
                actionMessage = if (text.isBlank()) "Título removido." else "Título salvo."
                loadTitles()
            } catch (error: Throwable) {
                actionIsError = true
                actionMessage = error.readable()
            }
        }
    }

    fun chooseQueue(next: Queue) {
        selection = emptySet()
        if (next != queue) counts = null
        queue = next
        if (next != Queue.SUPPORT && mode == ListMode.Normal) searchText = ""
        refresh()
    }

    /** Escolhe a situação a mostrar na aba: ativos, adiados ou arquivados. */
    fun chooseMode(next: ListMode) {
        selection = emptySet()
        mode = next
        searchText = ""
        refresh()
    }

    /** Marca ou desmarca um chamado na seleção em lote. */
    fun toggleSelection(id: String) {
        selection = if (id in selection) selection - id else selection + id
    }

    /** Seleciona todos os chamados que a lista mostra agora. */
    fun selectAll() {
        selection = tickets.map { it.id }.toSet()
    }

    fun clearSelection() {
        selection = emptySet()
    }

    /**
     * Aplica uma ação a todos os chamados escolhidos, um a um; um que falha não impede os outros. Ao
     * fim, o resumo diz quantos deram certo e por que os outros falharam, e só os que falharam ficam
     * selecionados (para tentar de novo).
     */
    fun runBatch(action: BatchAction, text: String? = null, tags: List<String> = emptyList(), until: ULong? = null) {
        val chosen = tickets.map { it.id }.filter { it in selection }
        if (chosen.isEmpty() || batchBusy) return
        viewModelScope.launch {
            batchBusy = true
            batchMessage = null
            val failures = mutableListOf<Pair<String, String>>()
            try {
                val core = open()
                for (id in chosen) {
                    try {
                        when (action) {
                            BatchAction.Archive -> core.setArchived(id, true)
                            BatchAction.Unarchive -> core.setArchived(id, false)
                            BatchAction.Snooze -> core.snooze(id, until!!)
                            BatchAction.Comment, BatchAction.Note -> {
                                val kind = if (action == BatchAction.Comment) ShareKind.COMMENT else ShareKind.INTERNAL_NOTE
                                val job = ShareJob(
                                    kind = kind, serviceId = null, campus = null, center = null, interested = null,
                                    ticket = id, text = text, title = null, filePaths = emptyList(), resumeFrom = 0u,
                                )
                                val outcome = core.applyShare(job)
                                if (!outcome.done) failures += id to describe(outcome.steps)
                            }
                            BatchAction.AddTags -> core.addTags(id, tags)
                        }
                    } catch (error: Throwable) {
                        failures += id to error.readable()
                    }
                }
                val done = chosen.size - failures.size
                batchIsError = failures.isNotEmpty()
                batchMessage = buildString {
                    append("${action.label}: $done de ${chosen.size} chamados.")
                    failures.forEach { (id, why) -> append("\n#$id: $why") }
                }
                selection = failures.map { it.first }.toSet()
                listStale = true
                loadTitles()
                refresh()
            } catch (error: Throwable) {
                batchIsError = true
                batchMessage = error.readable()
            } finally {
                batchBusy = false
            }
        }
    }

    /** Os filtros da lista da aba e da situação em uso, na página pedida (`null` = a primeira). */
    private fun listQuery(page: UInt?): ListQuery {
        val text = searchText.trim().ifEmpty { null }
        // Nos arquivados e nos adiados a busca é pelo título local, e todas as situações entram.
        val view = when (mode) {
            ListMode.Normal -> ListView.NORMAL
            ListMode.Archived -> ListView.ARCHIVED
            ListMode.Snoozed -> ListView.SNOOZED
        }
        val marked = mode != ListMode.Normal
        return ListQuery(
            title = if (marked) text else null, view = view, text = if (marked) null else text,
            statuses = emptyList(), allStatuses = marked && queue != Queue.TO_CLOSE, since = null, until = null, page = page,
        )
    }

    fun refresh() {
        val generation = ++listGeneration
        viewModelScope.launch {
            listLoading = true
            listError = null
            try {
                tickets = open().list(queue, listQuery(null))
                nextPage = 2u
                endReached = false
                listStale = false
                loadPhotos(tickets)
                // Os selos não atrapalham a lista: se a contagem falhar, ficam como estavam.
                val text = searchText.trim().ifEmpty { null }
                val base = ListQuery(
                    title = null, view = ListView.NORMAL, text = if (mode != ListMode.Normal) null else text,
                    statuses = emptyList(), allStatuses = false, since = null, until = null, page = null,
                )
                runCatching { open().listCounts(queue, base) }.onSuccess { if (generation == listGeneration) counts = it }
            } catch (error: Throwable) {
                if (!stillLoggedIn()) screen = Screen.Login else listError = error.readable()
            } finally {
                listLoading = false
            }
        }
    }

    /**
     * Carrega a próxima página ao rolar até o fim, sem repetir chamados. Uma página em que tudo já estava
     * na lista (ou estava arquivado/adiado) não conta: tenta a seguinte, até algumas vezes. Uma página
     * além do fim (o SUAP recusa ou devolve vazio) encerra a lista.
     */
    fun loadMore() {
        if (listLoading || loadingMore || endReached || tickets.isEmpty() || listError != null) return
        val generation = listGeneration
        viewModelScope.launch {
            loadingMore = true
            try {
                var added = 0
                var tries = 0
                while (added == 0 && tries < MAX_EMPTY_PAGES && !endReached) {
                    val page = nextPage
                    val more = try {
                        open().list(queue, listQuery(page))
                    } catch (error: Throwable) {
                        if (!stillLoggedIn()) { screen = Screen.Login; return@launch }
                        emptyList()
                    }
                    if (generation != listGeneration) return@launch
                    val known = tickets.map { it.id }.toSet()
                    val fresh = more.filter { it.id !in known }
                    if (fresh.isNotEmpty()) tickets = tickets + fresh
                    added = fresh.size
                    nextPage = page + 1u
                    tries++
                    // Sem nada de novo e sem nada na página: acabou. Páginas só de ocultos seguem adiante.
                    if (more.isEmpty()) endReached = true
                    loadPhotos(fresh)
                }
                // Cinco páginas seguidas sem nada novo: a lista termina aqui.
                if (added == 0) endReached = true
            } finally {
                loadingMore = false
            }
        }
    }

    /** Baixa (uma vez) as fotos de quem pediu; a lista não espera por elas. */
    private fun loadPhotos(list: List<uniffi.chamados_mobile.TicketSummary>) = loadPhotoPaths(list.mapNotNull { it.photo })

    /** As fotos do chamado: a do interessado e as de quem escreveu cada comentário, nota e acontecimento. */
    private fun detailPhotos(view: uniffi.chamados_mobile.TicketView): List<String> =
        listOfNotNull(view.photo) + (view.thread + view.notes).mapNotNull { it.photo }

    fun loadPhotoPaths(paths: List<String>) {
        val wanted = paths.distinct().filter { it !in photoFiles }
        if (wanted.isEmpty()) return
        viewModelScope.launch {
            val found = wanted.mapNotNull { path -> runCatching { open().ticketPhoto(path) }.getOrNull()?.let { path to it } }
            if (found.isNotEmpty()) photoFiles = photoFiles + found
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
                detail?.let { loadPhotoPaths(detailPhotos(it)) }
                // Ver um chamado que voltou do adiamento tira a marca de "novo" (em todos os aparelhos).
                if (snoozes[id]?.due == true) {
                    open().unsnooze(id)
                    listStale = true
                    loadTitles()
                }
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
        runCatching { open().show(id) }.onSuccess { detail = it; loadPhotoPaths(detailPhotos(it)) }
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
            user = null
            tickets = emptyList()
            screen = Screen.Login
        }
    }

    fun back() {
        when (screen) {
            is Screen.Detail -> {
                screen = Screen.Tickets
                if (listStale) refresh()
            }
            Screen.NewTicket -> screen = Screen.Tickets
            Screen.Settings -> start()
            Screen.Sync -> screen = Screen.Settings
            Screen.Documents -> screen = Screen.Tickets
            else -> Unit
        }
    }

    fun openSettings() {
        screen = Screen.Settings
    }

    /** Roda uma operação da sincronização e mostra o resultado; `done` diz o que foi feito. */
    private fun syncTask(done: String?, block: suspend (Chamados) -> String?) {
        viewModelScope.launch {
            syncBusy = true
            syncMessage = null
            try {
                val result = block(open())
                syncIsError = false
                syncMessage = result ?: done
            } catch (error: Throwable) {
                syncIsError = true
                syncMessage = error.readable()
            } finally {
                runCatching { syncState = open().syncState() }
                syncBusy = false
            }
        }
    }

    fun openDocuments() {
        screen = Screen.Documents
    }

    fun openSync() {
        screen = Screen.Sync
        shownKey = null
        syncMessage = null
        syncTask(null) { null }
    }

    fun saveBucket(setup: SyncSetup) = syncTask("Bucket guardado.") { it.syncSetup(setup); null }

    fun saveCredentials(id: String, secret: String) =
        syncTask("Credenciais guardadas.") { it.syncSetCredentials(id, secret); syncIfReady(it) }

    /** Aplica o texto de um pareamento (o do QR); a chave existente só é trocada com `replace`. */
    fun applyPairing(text: String, replace: Boolean) = syncTask("Pareado: bucket e chave guardados. Falta só informar as credenciais.") {
        check(replace || it.syncState().hasKey.not()) { "Já existe uma chave neste aparelho." }
        it.syncApplyPairing(text)
        syncIfReady(it)
    }

    fun generateKey(replace: Boolean) = syncTask("Chave criada.") { it.syncGenerateKey(replace); null }

    fun importKey(hex: String, replace: Boolean) = syncTask("Chave guardada.") { it.syncImportKey(hex, replace); null }

    fun revealKey() = syncTask(null) {
        shownKey = it.syncKeyHex()
        null
    }

    fun syncNow() = syncTask(null) { runSync(it) }

    /** Sincroniza na hora se já há bucket, chave e credenciais (o que falta o QR e a digitação completam). */
    private suspend fun syncIfReady(core: Chamados): String? {
        val state = core.syncState()
        return if (state.configured && state.hasKey && state.hasCredentials) runSync(core) else null
    }

    private suspend fun runSync(core: Chamados): String {
        val report = core.syncNow()
        titles = core.titles().associate { item -> item.id to item.title }
        return "Sincronizado: ${report.profiles} perfil(is), ${report.localChanges} alteração(ões) aqui, nuvem " +
            if (report.uploaded) "atualizada." else "já estava em dia."
    }

    fun forgetSync() = syncTask("Sincronização desligada neste aparelho.") { it.syncForget(); shownKey = null; null }

    fun changeBaseUrl(url: String) {
        val clean = Profiles.cleanUrl(url)
        if (clean == null) {
            listError = "Endereço inválido: use http:// ou https://"
            return
        }
        CoreAccess.saveBaseUrl(getApplication(), clean)
        reloadProfile()
    }
}
