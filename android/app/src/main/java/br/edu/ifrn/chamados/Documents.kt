package br.edu.ifrn.chamados

import android.content.Context
import android.net.Uri
import android.provider.OpenableColumns
import androidx.work.CoroutineWorker
import androidx.work.ExistingWorkPolicy
import androidx.work.OneTimeWorkRequestBuilder
import androidx.work.WorkManager
import androidx.work.WorkerParameters
import androidx.work.workDataOf
import java.io.File
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.withContext
import uniffi.chamados_mobile.DocInfo
import uniffi.chamados_mobile.DocState
import uniffi.chamados_mobile.Docs
import uniffi.chamados_mobile.DraftReference
import uniffi.chamados_mobile.IndexControl
import uniffi.chamados_mobile.TicketView

/** O tamanho máximo de um documento importado (o núcleo aplica o mesmo limite). */
const val MAX_DOCUMENT_BYTES = 50L * 1024 * 1024

/** Só PDF, TXT e Markdown. */
fun isSupportedDocument(name: String): Boolean {
    val lower = name.lowercase()
    return listOf(".pdf", ".txt", ".text", ".md", ".markdown").any { lower.endsWith(it) }
}

/** O documento ainda precisa ser indexado (na fila ou em andamento)? */
fun DocInfo.needsIndexing(): Boolean = state == DocState.PENDING || state == DocState.INDEXING

/** O estado do documento em uma frase para a tela. */
fun DocInfo.statusText(): String = when (state) {
    DocState.PENDING -> if (indexedPages > 0u) "Parado em ${indexedPages} de ${pages ?: 0u} páginas; continua sozinho." else "Na fila para indexar."
    DocState.INDEXING -> pages?.let { "Indexando… página $indexedPages de $it" } ?: "Indexando…"
    DocState.READY -> "Pronto: $chunks trechos."
    DocState.NO_TEXT -> "Sem texto para ler (um PDF digitalizado, por exemplo: o app não faz OCR)."
    DocState.PROTECTED -> "PDF protegido por senha: o app não o lê."
    DocState.CORRUPTED -> "Arquivo inválido ou danificado."
    DocState.TOO_LARGE -> "Grande demais (no máximo 50 MB e 500 páginas)."
}

/**
 * Os documentos de apoio do assistente. Ficam em pasta privada do app, **fora do backup automático** (que tem
 * limite de 25 MB e deixaria de funcionar com PDFs grandes), uma base por perfil.
 */
object Documents {
    /** Os tipos que o seletor de arquivos mostra (PDF e texto; o que mais passar é recusado pelo nome). */
    val mimeTypes = arrayOf("application/pdf", "text/*", "application/octet-stream")

    fun dir(context: Context): File =
        File(context.noBackupFilesDir, "docs-" + Profiles.active(context).dir).apply { mkdirs() }

    fun open(context: Context): Docs = Docs.open(dir(context).absolutePath)

    /** Copia o arquivo escolhido para uma cópia temporária e devolve o nome e a cópia. */
    fun copyToTemp(context: Context, uri: Uri): Pair<String, File>? {
        val resolver = context.contentResolver
        val name = resolver.query(uri, arrayOf(OpenableColumns.DISPLAY_NAME), null, null, null)?.use { cursor ->
            if (cursor.moveToFirst()) cursor.getString(0) else null
        } ?: uri.lastPathSegment ?: return null
        val temp = File.createTempFile("doc", ".tmp", context.cacheDir)
        resolver.openInputStream(uri)?.use { input -> temp.outputStream().use { input.copyTo(it) } } ?: return null
        return name to temp
    }

    /**
     * Os trechos dos documentos que podem ajudar no texto deste chamado (vazio se não há documento ou nada
     * casa). A busca parte do que a pessoa digitou, do título e do tipo do chamado.
     */
    suspend fun references(context: Context, ticket: TicketView, said: String?, title: String?): List<DraftReference> =
        withContext(Dispatchers.IO) {
            val docs = open(context)
            if (docs.list().none { it.state == DocState.READY }) return@withContext emptyList()
            val query = listOfNotNull(said, title, ticket.heading ?: ticket.title).joinToString(" ")
            docs.search(query, 3u).map { DraftReference(source = it.source, page = it.page, text = it.text) }
        }

    /** Enfileira a indexação (uma por documento; se já há uma, não duplica). */
    fun enqueue(context: Context, id: Long) {
        val request = OneTimeWorkRequestBuilder<IndexWorker>()
            .setInputData(workDataOf(IndexWorker.DIR to dir(context).absolutePath, IndexWorker.DOC to id))
            .build()
        WorkManager.getInstance(context).enqueueUniqueWork(workName(context, id), ExistingWorkPolicy.KEEP, request)
    }

    fun cancel(context: Context, id: Long) {
        WorkManager.getInstance(context).cancelUniqueWork(workName(context, id))
    }

    private fun workName(context: Context, id: Long) = "index-${Profiles.active(context).dir}-$id"
}

/**
 * Indexa um documento em segundo plano. O progresso vai para o banco página a página (a tela o lê de lá); se o
 * processo morrer no meio, a próxima rodada continua da última página gravada.
 */
class IndexWorker(context: Context, params: WorkerParameters) : CoroutineWorker(context, params) {
    override suspend fun doWork(): Result {
        val id = inputData.getLong(DOC, -1L)
        val dir = inputData.getString(DIR) ?: return Result.failure()
        if (id < 0) return Result.failure()
        val control = IndexControl()
        // A indexação roda em código nativo e não vê o cancelamento da corrotina: avisa o controle.
        kotlin.coroutines.coroutineContext[Job]?.invokeOnCompletion { error -> if (error != null) control.cancel() }
        return try {
            withContext(Dispatchers.IO) { Docs.open(dir).indexDocument(id, control) }
            Result.success()
        } catch (error: Exception) {
            Result.failure()
        }
    }

    companion object {
        const val DIR = "dir"
        const val DOC = "doc"
    }
}
