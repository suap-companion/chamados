package br.edu.ifrn.chamados

import android.app.Application
import androidx.activity.compose.BackHandler
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Button
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.LinearProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import androidx.lifecycle.AndroidViewModel
import androidx.lifecycle.viewModelScope
import androidx.lifecycle.viewmodel.compose.viewModel
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import uniffi.chamados_mobile.DocInfo
import uniffi.chamados_mobile.DocState

/** Os documentos de apoio da tela: importar, acompanhar a indexação, reindexar e remover. */
class DocumentsViewModel(application: Application) : AndroidViewModel(application) {
    var documents by mutableStateOf<List<DocInfo>>(emptyList())
        private set
    var message by mutableStateOf<String?>(null)

    private val context get() = getApplication<Application>()

    /** Lê a lista e põe na fila o que ficou parado (por exemplo, depois de o processo ser encerrado). */
    fun refresh() {
        viewModelScope.launch {
            try {
                val list = withContext(Dispatchers.IO) { Documents.open(context).list() }
                documents = list
                list.filter { it.needsIndexing() }.forEach { Documents.enqueue(context, it.id) }
            } catch (error: Exception) {
                message = "Não foi possível ler os documentos. Tente de novo."
            }
        }
    }

    fun import(uris: List<android.net.Uri>) {
        viewModelScope.launch {
            val notes = mutableListOf<String>()
            withContext(Dispatchers.IO) {
                val docs = Documents.open(context)
                for (uri in uris) {
                    val copied = Documents.copyToTemp(context, uri)
                    if (copied == null) {
                        notes += "Não foi possível ler um dos arquivos."
                        continue
                    }
                    val (name, temp) = copied
                    try {
                        when {
                            !isSupportedDocument(name) -> notes += "$name: use PDF, TXT ou Markdown."
                            temp.length() > MAX_DOCUMENT_BYTES -> notes += "$name: passa de 50 MB."
                            else -> Documents.enqueue(context, docs.importFile(name, temp.absolutePath).toLong())
                        }
                    } catch (error: Exception) {
                        notes += "$name: não foi possível importar."
                    } finally {
                        temp.delete()
                    }
                }
            }
            message = notes.joinToString("\n").ifEmpty { null }
            refresh()
        }
    }

    fun cancel(id: Long) {
        Documents.cancel(context, id)
        refresh()
    }

    fun reindex(id: Long) {
        viewModelScope.launch {
            withContext(Dispatchers.IO) { Documents.open(context).reindex(id) }
            Documents.enqueue(context, id)
            refresh()
        }
    }

    fun remove(id: Long) {
        viewModelScope.launch {
            Documents.cancel(context, id)
            withContext(Dispatchers.IO) { Documents.open(context).remove(id) }
            refresh()
        }
    }
}

/** A tela "Documentos de apoio". */
@Composable
fun DocumentsScreen(model: MainViewModel) {
    BackHandler { model.back() }
    val docs: DocumentsViewModel = viewModel()
    var removing by remember { mutableStateOf<DocInfo?>(null) }
    val picker = rememberLauncherForActivityResult(ActivityResultContracts.OpenMultipleDocuments()) { uris ->
        if (uris.isNotEmpty()) docs.import(uris)
    }
    // Acompanha a indexação lendo o banco (o worker grava página a página).
    LaunchedEffect(Unit) {
        while (true) {
            docs.refresh()
            delay(1500)
        }
    }
    Scaffold(topBar = { GovBrHeader("Documentos de apoio", onBack = model::back) }) { padding ->
        LazyColumn(
            Modifier.padding(padding),
            contentPadding = androidx.compose.foundation.layout.PaddingValues(16.dp),
            verticalArrangement = Arrangement.spacedBy(12.dp),
        ) {
            item {
                Text(
                    "Importe procedimentos, modelos e respostas prontas (PDF, TXT ou Markdown). O assistente usa só os " +
                        "trechos mais relevantes como apoio ao escrever e cita o arquivo e a página. Os documentos ficam só " +
                        "neste aparelho, em pasta privada do app, fora do backup; nunca vão ao SUAP nem à sincronização.",
                    style = MaterialTheme.typography.bodySmall,
                )
            }
            item {
                Button(onClick = { picker.launch(Documents.mimeTypes) }, modifier = Modifier.fillMaxWidth()) {
                    Text("Importar documentos")
                }
            }
            docs.message?.let { note -> item { Text(note, color = MaterialTheme.colorScheme.error) } }
            if (docs.documents.isEmpty()) {
                item { Text("Nenhum documento ainda.", style = MaterialTheme.typography.bodyLarge) }
            }
            items(docs.documents, key = { it.id }) { document -> DocumentRow(document, docs) { removing = document } }
        }
    }
    removing?.let { document ->
        AlertDialog(
            onDismissRequest = { removing = null },
            title = { Text("Remover o documento?") },
            text = { Text("${document.name} sai do aparelho junto com os trechos indexados.") },
            confirmButton = { TextButton(onClick = { removing = null; docs.remove(document.id) }) { Text("Remover") } },
            dismissButton = { TextButton(onClick = { removing = null }) { Text("Voltar") } },
        )
    }
}

@Composable
private fun DocumentRow(document: DocInfo, docs: DocumentsViewModel, onRemove: () -> Unit) {
    Column(verticalArrangement = Arrangement.spacedBy(4.dp)) {
        Text(document.name, style = MaterialTheme.typography.titleSmall)
        Text(
            "${document.kind.uppercase()} · ${formatSize(document.size.toLong())}",
            style = MaterialTheme.typography.labelMedium,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
        Text(document.statusText(), style = MaterialTheme.typography.bodyMedium)
        val pages = document.pages
        if (document.needsIndexing() && pages != null && pages > 0u) {
            LinearProgressIndicator(
                progress = { document.indexedPages.toFloat() / pages.toFloat() },
                modifier = Modifier.fillMaxWidth(),
            )
        }
        Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            if (document.needsIndexing()) {
                TextButton(onClick = { docs.cancel(document.id) }) { Text("Pausar") }
            } else {
                TextButton(onClick = { docs.reindex(document.id) }) { Text("Reindexar") }
            }
            TextButton(onClick = onRemove) { Text("Remover") }
        }
        HorizontalDivider()
    }
}
