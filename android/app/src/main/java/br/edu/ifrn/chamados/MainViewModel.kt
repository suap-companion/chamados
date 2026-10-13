package br.edu.ifrn.chamados

import android.app.Application
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.lifecycle.AndroidViewModel
import androidx.lifecycle.viewModelScope
import java.io.File
import kotlinx.coroutines.launch
import uniffi.chamados_mobile.Chamados
import uniffi.chamados_mobile.ChamadosException
import uniffi.chamados_mobile.ListQuery
import uniffi.chamados_mobile.Queue
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
