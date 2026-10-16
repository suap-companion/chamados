package br.edu.ifrn.chamados

import android.annotation.SuppressLint
import android.content.Context
import android.content.Intent
import android.webkit.CookieManager
import android.webkit.MimeTypeMap
import android.webkit.WebResourceRequest
import android.webkit.WebView
import android.webkit.WebViewClient
import androidx.activity.compose.BackHandler
import androidx.compose.foundation.clickable
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.width
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Add
import androidx.compose.material3.FloatingActionButton
import androidx.compose.material3.Icon
import androidx.compose.material3.Tab
import androidx.compose.material3.TabRow
import androidx.compose.material3.pulltorefresh.PullToRefreshBox
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.res.painterResource
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.foundation.lazy.items
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Badge
import androidx.compose.material3.Button
import androidx.compose.material3.Card
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
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.unit.dp
import androidx.compose.ui.viewinterop.AndroidView
import androidx.core.content.FileProvider
import java.io.File
import java.net.URI
import kotlinx.coroutines.launch
import uniffi.chamados_mobile.Queue
import uniffi.chamados_mobile.TicketView

@Composable
fun ChamadosApp(model: MainViewModel) {
    when (val screen = model.screen) {
        Screen.Start -> Splash()
        Screen.Login -> LoginScreen(model)
        Screen.Tickets -> TicketsScreen(model)
        is Screen.Detail -> DetailScreen(model, screen.id)
        Screen.Settings -> SettingsScreen(model)
        Screen.NewTicket -> NewTicketScreen(model)
        Screen.Sync -> SyncScreen(model)
    }
}

@Composable
private fun Centered(content: @Composable () -> Unit) {
    Column(
        Modifier.fillMaxSize(),
        verticalArrangement = Arrangement.Center,
        horizontalAlignment = Alignment.CenterHorizontally,
    ) { content() }
}

/** Login no próprio site do SUAP: o app só recebe os cookies, nunca a senha. */
@SuppressLint("SetJavaScriptEnabled")
@Composable
fun LoginScreen(model: MainViewModel) {
    val scope = rememberCoroutineScope()
    val base = model.baseUrl
    var message by remember { mutableStateOf<String?>(null) }
    var entering by remember { mutableStateOf(false) }
    Scaffold(topBar = { GovBrHeader("Entrar no SUAP", quick = listOf(HeaderItem("Endereço do SUAP") { model.openSettings() })) }) { padding ->
        Column(Modifier.padding(padding)) {
            message?.let { Text(it, Modifier.padding(16.dp), color = MaterialTheme.colorScheme.error) }
            if (entering) Centered { CircularProgressIndicator() }
            AndroidView(
                modifier = if (entering) Modifier else Modifier.fillMaxSize(),
                factory = { context ->
                    WebView(context).apply {
                        settings.javaScriptEnabled = true
                        settings.domStorageEnabled = true
                        webViewClient = object : WebViewClient() {
                            override fun shouldOverrideUrlLoading(view: WebView, request: WebResourceRequest) = false

                            override fun onPageFinished(view: WebView, url: String) {
                                if (!isAfterLogin(base, url)) return
                                entering = true
                                val cookies = CookieManager.getInstance().getCookie(base) ?: return
                                scope.launch {
                                    try {
                                        model.session().importSessionCookies(cookies)
                                        if (model.session().isAuthenticated()) model.loggedIn()
                                    } catch (error: Throwable) {
                                        entering = false
                                        message = error.readable()
                                    }
                                }
                            }
                        }
                        loadUrl(base + "accounts/login/?next=/centralservicos/meus_chamados/")
                    }
                },
            )
        }
    }
}

/** Já saiu da tela de login, ainda no site do SUAP (o login do gov.br fica em outro endereço). */
private fun isAfterLogin(base: String, url: String): Boolean {
    val (site, page) = runCatching { URI(base) to URI(url) }.getOrNull() ?: return false
    return page.authority == site.authority && !page.path.startsWith("/accounts/")
}

/** A tela de carregamento: o logo do app (docs/logo.png) enquanto a sessão é conferida. */
@Composable
private fun Splash() {
    Box(Modifier.fillMaxSize().background(Color(0xFFF8FAF9)), contentAlignment = Alignment.Center) {
        Image(painterResource(R.drawable.splash_logo), contentDescription = "Chamados", modifier = Modifier.width(280.dp))
    }
}

/**
 * A tela inicial: as três abas (Meus chamados, Fila de suporte, A fechar), a situação dentro de cada uma
 * (ativos, adiados, arquivados), o botão `+` para abrir um chamado e puxar a lista para atualizar.
 */
@OptIn(ExperimentalLayoutApi::class, ExperimentalMaterial3Api::class)
@Composable
fun TicketsScreen(model: MainViewModel) {
    val tabs = listOf(Queue.MINE to "Meus", Queue.SUPPORT to "Suporte", Queue.TO_CLOSE to "A fechar")
    val modes = listOf(ListMode.Normal to "Ativos", ListMode.Snoozed to "Adiados", ListMode.Archived to "Arquivados")
    val searchable = model.queue == Queue.SUPPORT || model.mode != ListMode.Normal
    var newProfile by remember { mutableStateOf(false) }
    Scaffold(
        topBar = {
            GovBrHeader(
                title = "Chamados",
                subtitle = model.activeProfile.name,
                quick = listOf(
                    HeaderItem("Configurações") { model.openSettings() },
                    HeaderItem("Sincronização entre dispositivos") { model.openSync() },
                ),
                search = if (searchable) {
                    HeaderSearch(
                        label = if (model.mode != ListMode.Normal) "Buscar no título local" else "Buscar no texto dos chamados",
                        value = model.searchText,
                        onChange = { model.searchText = it },
                        onSubmit = { model.refresh() },
                        onClose = {
                            model.searchText = ""
                            model.refresh()
                        },
                    )
                } else {
                    null
                },
                account = HeaderAccount(
                    name = model.user?.name,
                    enrollment = model.user?.enrollment,
                    profile = model.activeProfile.name,
                    photo = model.user?.photo,
                    items = model.profiles.filter { it.name != model.activeProfile.name }
                        .map { other -> HeaderItem("Trocar para ${other.name}") { model.switchProfile(other.name) } } +
                        listOf(
                            HeaderItem("Cadastrar novo perfil…") { newProfile = true },
                            HeaderItem("Sair (esquecer a sessão)") { model.logout() },
                        ),
                ),
            )
        },
        floatingActionButton = {
            FloatingActionButton(onClick = model::openNewTicket) { Icon(Icons.Filled.Add, contentDescription = "Novo chamado") }
        },
    ) { padding ->
        Column(Modifier.padding(padding)) {
            TabRow(selectedTabIndex = tabs.indexOfFirst { it.first == model.queue }.coerceAtLeast(0)) {
                tabs.forEach { (queue, label) ->
                    Tab(selected = model.queue == queue, onClick = { model.chooseQueue(queue) }, text = { Text(label, maxLines = 1) })
                }
            }
            FlowRow(Modifier.padding(horizontal = 12.dp), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                modes.forEach { (mode, label) ->
                    val amount = model.counts?.let {
                        when (mode) {
                            ListMode.Normal -> it.active
                            ListMode.Snoozed -> it.snoozed
                            ListMode.Archived -> it.archived
                        }
                    }?.toInt() ?: 0
                    FilterChip(
                        selected = model.mode == mode,
                        onClick = { model.chooseMode(mode) },
                        label = {
                            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                                Text(label)
                                if (amount > 0) Badge { Text(if (amount > 99) "99+" else amount.toString()) }
                            }
                        },
                    )
                }
            }
            PullToRefreshBox(isRefreshing = model.listLoading, onRefresh = model::refresh, modifier = Modifier.fillMaxSize()) {
                LazyColumn(
                    modifier = Modifier.fillMaxSize(),
                    contentPadding = PaddingValues(start = 12.dp, end = 12.dp, top = 12.dp, bottom = 88.dp),
                    verticalArrangement = Arrangement.spacedBy(8.dp),
                ) {
                    val error = model.listError
                    when {
                        error != null -> item { Message(error) }
                        model.tickets.isEmpty() && !model.listLoading -> item {
                            Message(
                                when (model.mode) {
                                    ListMode.Archived -> "Nenhum chamado arquivado nesta aba."
                                    ListMode.Snoozed -> "Nenhum chamado adiado nesta aba."
                                    ListMode.Normal -> "Nenhum chamado nesta aba. Puxe a lista para baixo para atualizar."
                                },
                            )
                        }
                        else -> items(model.tickets, key = { it.id }) { ticket ->
                            Card(Modifier.fillMaxWidth().clickable { model.openDetail(ticket.id) }) {
                                Column(Modifier.padding(12.dp)) {
                                    Text("#${ticket.id}  ${ticket.status.orEmpty()}", style = MaterialTheme.typography.labelLarge)
                                    model.snoozes[ticket.id]?.let { snooze ->
                                        val note = if (snooze.due) "↩ voltou do adiamento" else "⏰ volta ${formatMoment(snooze.until)}"
                                        Text(note, style = MaterialTheme.typography.labelMedium, color = MaterialTheme.colorScheme.primary)
                                    }
                                    model.titles[ticket.id]?.let { Text(it, style = MaterialTheme.typography.titleMedium) }
                                    Text(ticket.subject ?: "(sem assunto)", style = MaterialTheme.typography.bodyLarge)
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    if (newProfile) NewProfileDialog(model) { newProfile = false }
}

/** Cadastra um perfil (ambiente): o nome e o endereço do SUAP; passa a usá-lo e pede o login dele. */
@Composable
private fun NewProfileDialog(model: MainViewModel, onClose: () -> Unit) {
    var name by remember { mutableStateOf("") }
    var url by remember { mutableStateOf("https://") }
    AlertDialog(
        onDismissRequest = { model.profileError = null; onClose() },
        title = { Text("Cadastrar novo perfil") },
        text = {
            Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                OutlinedTextField(value = name, onValueChange = { name = it }, label = { Text("Nome do perfil") }, singleLine = true, modifier = Modifier.fillMaxWidth())
                OutlinedTextField(value = url, onValueChange = { url = it }, label = { Text("Endereço do SUAP") }, singleLine = true, modifier = Modifier.fillMaxWidth())
                model.profileError?.let { Text(it, color = MaterialTheme.colorScheme.error) }
            }
        },
        confirmButton = { TextButton(onClick = { if (model.addProfile(name, url)) onClose() }) { Text("Cadastrar") } },
        dismissButton = { TextButton(onClick = { model.profileError = null; onClose() }) { Text("Cancelar") } },
    )
}

@Composable
private fun Message(text: String) {
    Text(text, Modifier.padding(16.dp), style = MaterialTheme.typography.bodyLarge)
}

@Composable
fun DetailScreen(model: MainViewModel, id: String) {
    BackHandler { model.back() }
    val context = LocalContext.current
    val downloaded = model.downloaded
    LaunchedEffect(downloaded) {
        downloaded?.let { openFile(context, it.path) }
        model.downloaded = null
    }
    val exported = model.exported
    LaunchedEffect(exported) {
        exported?.let { shareZip(context, it.path) }
        model.exported = null
    }
    Scaffold(topBar = { GovBrHeader("Chamado #$id", onBack = model::back) }) { padding ->
        Column(Modifier.padding(padding)) {
            when {
                model.detailLoading -> Centered { CircularProgressIndicator() }
                model.detail != null -> Details(model, model.detail!!)
                else -> Message(model.detailError ?: "Sem dados.")
            }
        }
    }
}

@Composable
private fun Details(model: MainViewModel, view: TicketView) {
    LazyColumn(contentPadding = PaddingValues(16.dp), verticalArrangement = Arrangement.spacedBy(10.dp)) {
        item {
            Text(view.heading ?: view.title, style = MaterialTheme.typography.titleLarge)
            if (view.statuses.isNotEmpty()) Text(view.statuses.joinToString(" · "), style = MaterialTheme.typography.labelLarge)
            model.detailError?.let { Text(it, color = MaterialTheme.colorScheme.error) }
        }
        items(view.fields) { field ->
            Column {
                Text(field.label, style = MaterialTheme.typography.labelMedium)
                Text(field.value)
            }
        }
        item { TitleSection(model, view.id) }
        item { ActionsSection(model, view) }
        if (view.attachments.isNotEmpty()) {
            item { Text("Anexos", style = MaterialTheme.typography.titleMedium) }
            items(view.attachments) { attachment ->
                OutlinedButton(
                    onClick = { model.download(view.id, attachment.number) },
                    enabled = model.downloading == null,
                    modifier = Modifier.fillMaxWidth(),
                ) { Text(if (model.downloading == attachment.number) "Baixando…" else attachment.name) }
            }
        }
        if (view.timeline.isNotEmpty()) {
            item { Text("Linha do tempo", style = MaterialTheme.typography.titleMedium) }
            items(view.timeline) { entry ->
                Column {
                    Text(entry.date, style = MaterialTheme.typography.labelMedium)
                    Text(entry.text)
                    HorizontalDivider(Modifier.padding(top = 8.dp))
                }
            }
        }
    }
}

/** O título local do chamado (só seu, o SUAP não tem): entra na lista e na sincronização. */
@Composable
private fun TitleSection(model: MainViewModel, id: String) {
    var text by remember(id, model.titles[id]) { mutableStateOf(model.titles[id].orEmpty()) }
    Column(verticalArrangement = Arrangement.spacedBy(6.dp)) {
        OutlinedTextField(
            value = text,
            onValueChange = { text = it },
            label = { Text("Título local (só seu)") },
            singleLine = true,
            modifier = Modifier.fillMaxWidth(),
        )
        Button(onClick = { model.saveTitle(id, text) }, enabled = text.trim() != model.titles[id].orEmpty()) {
            Text(if (text.isBlank() && model.titles[id] != null) "Remover título" else "Salvar título")
        }
        val isArchived = id in model.archived
        OutlinedButton(onClick = { model.setArchived(id, !isArchived) }) {
            Text(if (isArchived) "Desarquivar" else "Arquivar (só no app e na sincronização)")
        }
        SnoozeSection(model, id)
        var asking by remember(id) { mutableStateOf(false) }
        OutlinedButton(onClick = { asking = true }, enabled = !model.exporting) {
            Text(if (model.exporting) "Gerando o ZIP…" else "Exportar como ZIP")
        }
        if (asking) {
            AlertDialog(
                onDismissRequest = { asking = false },
                title = { Text("Exportar o chamado #$id") },
                text = { Text("O ZIP leva o texto (chamado.md), os dados (chamado.json) e, se quiser, os anexos. Depois você escolhe para onde enviá-lo.") },
                confirmButton = { TextButton(onClick = { asking = false; model.exportTicket(id, true) }) { Text("Com anexos") } },
                dismissButton = {
                    Row {
                        TextButton(onClick = { asking = false; model.exportTicket(id, false) }) { Text("Sem anexos") }
                        TextButton(onClick = { asking = false }) { Text("Cancelar") }
                    }
                },
            )
        }
    }
}

/** Entrega o ZIP exportado ao aplicativo que a pessoa escolher (e-mail, nuvem, outro app...). */
private fun shareZip(context: Context, path: String) {
    val file = File(path)
    val uri = FileProvider.getUriForFile(context, "${context.packageName}.files", file)
    val intent = Intent(Intent.ACTION_SEND).setType("application/zip").putExtra(Intent.EXTRA_STREAM, uri)
        .addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION)
    runCatching { context.startActivity(Intent.createChooser(intent, file.name)) }
}

/** Abre o arquivo baixado no aplicativo que o usuário escolher. */
private fun openFile(context: Context, path: String) {
    val file = File(path)
    val uri = FileProvider.getUriForFile(context, "${context.packageName}.files", file)
    val extension = file.extension.lowercase()
    val mime = MimeTypeMap.getSingleton().getMimeTypeFromExtension(extension) ?: "application/octet-stream"
    val intent = Intent(Intent.ACTION_VIEW).setDataAndType(uri, mime).addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION)
    runCatching { context.startActivity(Intent.createChooser(intent, file.name)) }
}

@Composable
fun SettingsScreen(model: MainViewModel) {
    BackHandler { model.back() }
    var url by remember { mutableStateOf(model.baseUrl) }
    Scaffold(topBar = { GovBrHeader("Configurações", onBack = model::back) }) { padding ->
        Column(
            Modifier.padding(padding).padding(16.dp).verticalScroll(rememberScrollState()),
            verticalArrangement = Arrangement.spacedBy(12.dp),
        ) {
            model.listError?.let { Text(it, color = MaterialTheme.colorScheme.error) }
            OutlinedTextField(
                value = url,
                onValueChange = { url = it },
                label = { Text("Endereço do SUAP") },
                singleLine = true,
                modifier = Modifier.fillMaxWidth(),
            )
            Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                OutlinedButton(onClick = { url = PRODUCTION_URL }) { Text("IFRN") }
                OutlinedButton(onClick = { url = LOCAL_URL }) { Text("Local (emulador)") }
            }
            Button(onClick = { model.changeBaseUrl(url) }, modifier = Modifier.fillMaxWidth()) { Text("Salvar e entrar") }
            OutlinedButton(onClick = model::openSync, modifier = Modifier.fillMaxWidth()) { Text("Sincronização entre dispositivos") }
            OutlinedButton(onClick = model::logout, modifier = Modifier.fillMaxWidth()) { Text("Sair (esquecer a sessão)") }
            WatchSection()
            Text("Versão ${uniffi.chamados_mobile.coreVersion()}", style = MaterialTheme.typography.labelMedium)
        }
    }
}
