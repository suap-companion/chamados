package br.edu.ifrn.chamados

import android.app.DownloadManager
import android.content.Context
import android.net.Uri
import java.io.File
import java.security.MessageDigest
import java.util.Locale
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext

/** Um modelo que o app pode baixar: o arquivo, o tamanho e o SHA-256 publicados pela origem. */
data class ModelSpec(val file: String, val sizeBytes: Long, val sha256: String)

/**
 * O modelo local de reserva do assistente (opcional, baixado só com consentimento). É o Qwen2.5 1.5B
 * Instruct em `.litertlm` (quantizado em 8 bits) do repositório `litert-community`: licença Apache-2.0, sem
 * login nem aceite de termos (o Gemma, por exemplo, exige os dois e não dá para baixar de dentro do app).
 */
object LocalModel {
    const val NAME = "Qwen2.5 1.5B Instruct"
    const val LICENSE = "Apache-2.0"
    const val SOURCE = "huggingface.co/litert-community/Qwen2.5-1.5B-Instruct"
    const val FILE = "Qwen2.5-1.5B-Instruct_multi-prefill-seq_q8_ekv4096.litertlm"
    const val URL = "https://huggingface.co/litert-community/Qwen2.5-1.5B-Instruct/resolve/main/$FILE"

    /** Memória do aparelho abaixo da qual o app não oferece o download (o modelo usa ~2 GB ao rodar). */
    const val MIN_RAM_BYTES = 3_000_000_000L

    val SPEC = ModelSpec(
        file = FILE,
        sizeBytes = 1_597_922_856L,
        sha256 = "c4a9b7289365ab80fe599b844b77ad253eb3c53fb0e5d963d699146a6f01255e",
    )
}

/** Tamanho legível em português: "1,6 GB", "546 MB". */
fun formatSize(bytes: Long): String = when {
    bytes >= 1_000_000_000L -> String.format(Locale.forLanguageTag("pt-BR"), "%.1f GB", bytes / 1e9)
    bytes >= 1_000_000L -> "${bytes / 1_000_000} MB"
    else -> "$bytes B"
}

/** Cabe o download? Pede 20% de folga sobre o tamanho (arquivo parcial, verificação e o resto do aparelho). */
fun enoughSpace(freeBytes: Long, neededBytes: Long): Boolean = freeBytes >= neededBytes + neededBytes / 5

object Hashing {
    /** O SHA-256 do arquivo, em hexadecimal minúsculo, lido em blocos. */
    fun sha256(file: File): String {
        val digest = MessageDigest.getInstance("SHA-256")
        file.inputStream().use { input ->
            val buffer = ByteArray(1 shl 20)
            while (true) {
                val read = input.read(buffer)
                if (read < 0) break
                digest.update(buffer, 0, read)
            }
        }
        return digest.digest().joinToString("") { "%02x".format(it) }
    }
}

/** Os arquivos do modelo numa pasta: o completo e o que ainda está sendo baixado. */
class LocalModelFiles(dir: File, private val spec: ModelSpec = LocalModel.SPEC) {
    val model = File(dir, spec.file)
    val partial = File(dir, spec.file + ".download")

    /** O modelo está instalado: o arquivo completo existe e tem o tamanho certo. */
    fun isInstalled(): Boolean = model.isFile && model.length() == spec.sizeBytes

    /** Apaga o modelo e o que sobrou do download. */
    fun deleteAll() {
        model.delete()
        partial.delete()
    }

    /**
     * Confere o arquivo baixado (tamanho e SHA-256) e o instala; se não confere, apaga e devolve `false`.
     * O `sha` existe para os testes não precisarem de um arquivo de gigabytes.
     */
    fun verifyAndInstall(sha: (File) -> String = Hashing::sha256): Boolean {
        if (!partial.isFile) return false
        val ok = partial.length() == spec.sizeBytes && sha(partial).equals(spec.sha256, ignoreCase = true)
        if (!ok) {
            partial.delete()
            return false
        }
        model.delete()
        return partial.renameTo(model)
    }
}

/** Em que pé está o modelo local. */
sealed interface ModelState {
    data object Absent : ModelState
    data class Downloading(val done: Long, val total: Long) : ModelState
    data class Paused(val reason: String) : ModelState
    data object Verifying : ModelState
    data class Failed(val message: String) : ModelState
    data object Installed : ModelState
}

/**
 * Baixa o modelo pelo `DownloadManager` do sistema: ele retoma sozinho depois de queda de rede, segue os
 * redirecionamentos, mostra a notificação e continua com o app fechado. Depois, o app confere o SHA-256.
 * O arquivo fica na pasta externa do app (fora do backup e apagada com o app).
 */
class ModelDownloader(private val context: Context) {
    private val manager = context.getSystemService(DownloadManager::class.java)
    private val prefs = context.getSharedPreferences("chamados", Context.MODE_PRIVATE)
    private val dir: File? get() = context.getExternalFilesDir(null)?.let { File(it, "models") }
    private val files: LocalModelFiles? get() = dir?.let { LocalModelFiles(it) }

    /** Bytes livres onde o modelo vai ficar (0 se não há armazenamento externo). */
    fun freeBytes(): Long = dir?.let { it.parentFile?.freeSpace } ?: 0L

    /** Começa o download; `wifiOnly` impede o uso de dados móveis. */
    fun start(wifiOnly: Boolean) {
        val folder = dir ?: return
        folder.mkdirs()
        val target = LocalModelFiles(folder)
        target.partial.delete()
        val request = DownloadManager.Request(Uri.parse(LocalModel.URL))
            .setTitle("Modelo de IA do chamados")
            .setDescription("${LocalModel.NAME} (${formatSize(LocalModel.SPEC.sizeBytes)})")
            .setNotificationVisibility(DownloadManager.Request.VISIBILITY_VISIBLE_NOTIFY_COMPLETED)
            .setAllowedOverMetered(!wifiOnly)
            .setAllowedOverRoaming(false)
            .setDestinationInExternalFilesDir(context, null, "models/${target.partial.name}")
        prefs.edit().putLong(KEY, manager.enqueue(request)).apply()
    }

    /** Para o download e apaga o que veio; também apaga o modelo instalado. */
    fun remove() {
        val id = prefs.getLong(KEY, -1L)
        if (id >= 0) manager.remove(id)
        prefs.edit().remove(KEY).apply()
        files?.deleteAll()
    }

    fun state(): ModelState {
        if (files?.isInstalled() == true) return ModelState.Installed
        val id = prefs.getLong(KEY, -1L)
        if (id < 0) return ModelState.Absent
        val query = DownloadManager.Query().setFilterById(id)
        manager.query(query).use { cursor ->
            if (!cursor.moveToFirst()) {
                prefs.edit().remove(KEY).apply()
                return ModelState.Absent
            }
            val status = cursor.getInt(cursor.getColumnIndexOrThrow(DownloadManager.COLUMN_STATUS))
            val reason = cursor.getInt(cursor.getColumnIndexOrThrow(DownloadManager.COLUMN_REASON))
            val done = cursor.getLong(cursor.getColumnIndexOrThrow(DownloadManager.COLUMN_BYTES_DOWNLOADED_SO_FAR))
            return when (status) {
                DownloadManager.STATUS_SUCCESSFUL -> ModelState.Verifying
                DownloadManager.STATUS_FAILED -> {
                    prefs.edit().remove(KEY).apply()
                    ModelState.Failed(failure(reason))
                }
                DownloadManager.STATUS_PAUSED -> ModelState.Paused(pause(reason))
                else -> ModelState.Downloading(done, LocalModel.SPEC.sizeBytes)
            }
        }
    }

    /** Confere o arquivo baixado (lê gigabytes: fora da thread principal) e o instala. */
    suspend fun verify(): ModelState = withContext(Dispatchers.IO) {
        val installed = files?.verifyAndInstall() == true
        prefs.edit().remove(KEY).apply()
        if (installed) ModelState.Installed
        else ModelState.Failed("O arquivo baixado não confere com o publicado (SHA-256). Ele foi apagado; tente baixar de novo.")
    }

    private fun failure(reason: Int): String = when (reason) {
        DownloadManager.ERROR_INSUFFICIENT_SPACE -> "Não há espaço suficiente no aparelho."
        DownloadManager.ERROR_DEVICE_NOT_FOUND -> "O armazenamento do aparelho não está disponível."
        DownloadManager.ERROR_CANNOT_RESUME, DownloadManager.ERROR_HTTP_DATA_ERROR, DownloadManager.ERROR_UNKNOWN ->
            "O download falhou. Verifique a conexão e tente de novo."
        else -> "O download falhou (código $reason). Tente de novo."
    }

    private fun pause(reason: Int): String = when (reason) {
        DownloadManager.PAUSED_QUEUED_FOR_WIFI -> "Esperando uma rede Wi-Fi."
        DownloadManager.PAUSED_WAITING_FOR_NETWORK -> "Esperando a rede voltar."
        else -> "Em pausa; continua sozinho."
    }

    private companion object {
        const val KEY = "local_model_download_id"
    }
}
