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
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
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
        Screen.Start -> Centered { CircularProgressIndicator() }
        Screen.Login -> LoginScreen(model)
        Screen.Tickets -> TicketsScreen(model)
        is Screen.Detail -> DetailScreen(model, screen.id)
        Screen.Settings -> SettingsScreen(model)
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
    Scaffold(topBar = { Bar("Entrar no SUAP", actions = { TextButton(onClick = model::openSettings) { Text("Endereço") } }) }) { padding ->
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

@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun Bar(title: String, onBack: (() -> Unit)? = null, actions: @Composable () -> Unit = {}) {
    TopAppBar(
        title = { Text(title) },
        navigationIcon = { onBack?.let { TextButton(onClick = it) { Text("‹ Voltar") } } },
        actions = { actions() },
    )
}

@Composable
fun TicketsScreen(model: MainViewModel) {
    val labels = listOf(Queue.MINE to "Meus", Queue.SUPPORT to "Suporte", Queue.TO_CLOSE to "A fechar")
    Scaffold(
        topBar = {
            Bar("Chamados", actions = {
                TextButton(onClick = model::refresh) { Text("Atualizar") }
                TextButton(onClick = model::openSettings) { Text("Config.") }
            })
        },
    ) { padding ->
        Column(Modifier.padding(padding)) {
            Row(Modifier.padding(horizontal = 12.dp), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                labels.forEach { (queue, label) ->
                    FilterChip(selected = model.queue == queue, onClick = { model.chooseQueue(queue) }, label = { Text(label) })
                }
            }
            if (model.queue == Queue.SUPPORT) {
                OutlinedTextField(
                    value = model.searchText,
                    onValueChange = { model.searchText = it },
                    label = { Text("Buscar no texto (Enter)") },
                    singleLine = true,
                    keyboardActions = androidx.compose.foundation.text.KeyboardActions(onSearch = { model.refresh() }),
                    keyboardOptions = androidx.compose.foundation.text.KeyboardOptions(imeAction = androidx.compose.ui.text.input.ImeAction.Search),
                    modifier = Modifier.fillMaxWidth().padding(12.dp),
                )
            }
            when {
                model.listLoading -> Centered { CircularProgressIndicator() }
                model.listError != null -> Message(model.listError!!)
                model.tickets.isEmpty() -> Message("Nenhum chamado nesta lista.")
                else -> LazyColumn(contentPadding = PaddingValues(12.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                    items(model.tickets, key = { it.id }) { ticket ->
                        Card(Modifier.fillMaxWidth().clickable { model.openDetail(ticket.id) }) {
                            Column(Modifier.padding(12.dp)) {
                                Text("#${ticket.id}  ${ticket.status.orEmpty()}", style = MaterialTheme.typography.labelLarge)
                                Text(ticket.subject ?: "(sem assunto)", style = MaterialTheme.typography.bodyLarge)
                            }
                        }
                    }
                }
            }
        }
    }
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
    Scaffold(topBar = { Bar("Chamado #$id", onBack = model::back) }) { padding ->
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
    Scaffold(topBar = { Bar("Configurações", onBack = model::back) }) { padding ->
        Column(Modifier.padding(padding).padding(16.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
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
            OutlinedButton(onClick = model::logout, modifier = Modifier.fillMaxWidth()) { Text("Sair (esquecer a sessão)") }
            Text("Versão ${uniffi.chamados_mobile.coreVersion()}", style = MaterialTheme.typography.labelMedium)
        }
    }
}
