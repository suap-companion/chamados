package br.edu.ifrn.chamados

import android.content.Context
import android.net.Uri
import android.provider.OpenableColumns
import android.webkit.MimeTypeMap
import java.io.File
import uniffi.chamados_mobile.RejectedFile
import uniffi.chamados_mobile.SharedFileInfo
import uniffi.chamados_mobile.classifyShared

/** Um arquivo recebido, já copiado para a pasta do app. */
data class ReceivedFile(val name: String, val path: String, val size: Long)

/** Copia o que veio de outro app (um `content://`) para a pasta do app e confere o que o SUAP aceita. */
object FileCopy {
    /** Copia um `content://` para `folder` (o app de origem pode revogar o acesso depois). */
    fun copy(context: Context, uri: Uri, folder: File, index: Int): ReceivedFile? {
        val resolver = context.contentResolver
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

    /** Os arquivos que o SUAP aceita e os que recusa (com o motivo). */
    fun classify(files: List<ReceivedFile>, limit: UInt? = null): Pair<List<ReceivedFile>, List<RejectedFile>> {
        val result = classifyShared(files.map { SharedFileInfo(it.name, it.size.toULong()) }, limit)
        val accepted = result.accepted.map { it.toInt() }.toSet()
        return files.filterIndexed { index, _ -> index in accepted } to result.rejected
    }
}
