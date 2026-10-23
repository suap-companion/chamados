package br.edu.ifrn.chamados

import android.app.DownloadManager
import android.content.Context
import com.google.mlkit.genai.common.DownloadStatus
import com.google.mlkit.genai.common.FeatureStatus
import com.google.mlkit.genai.common.GenAiException
import com.google.mlkit.genai.prompt.GenerateContentRequest
import com.google.mlkit.genai.prompt.Generation
import com.google.mlkit.genai.prompt.GenerativeModel
import com.google.mlkit.genai.prompt.TextPart
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.flow
import java.io.File

/** O estado da IA do aparelho (Gemini Nano via AICore). */
sealed interface AiState {
    /** Sem IA do sistema: Android antigo, aparelho sem AICore ou sem suporte. */
    data object Unsupported : AiState

    /** O modelo ainda não está no aparelho; dá para baixar. */
    data object Downloadable : AiState

    /** O download está em andamento. */
    data object Downloading : AiState

    data object Ready : AiState
}

/**
 * Quem escreve o rascunho. A regra do que entra no pedido e da limpeza da resposta fica no Rust
 * (`assistPrompt`/`assistClean`); aqui só se fala com o modelo, que roda no aparelho.
 */
interface AnswerProvider {
    suspend fun state(): AiState

    /** Baixa o modelo; emite os bytes já baixados e termina ao concluir (ou falha com uma exceção). */
    fun download(): Flow<Long>

    /** Gera o texto; cada valor emitido é o texto inteiro até agora. */
    fun generate(prompt: String): Flow<String>
}

/**
 * Escolhe o provedor na hora: o primeiro da lista que estiver pronto; se nenhum estiver, fica o estado do
 * primeiro (para o app oferecer o download ou o modelo pronto). Desligado nas configurações, nenhum é usado.
 * Hoje só há a IA do sistema; a lista deixa espaço para outro provedor, como a IA na nuvem com a chave da pessoa.
 */
class ChainProvider(
    private val providers: List<AnswerProvider>,
    private val enabled: () -> Boolean,
) : AnswerProvider {
    private var active: AnswerProvider = providers.first()

    override suspend fun state(): AiState {
        val first = providers.first()
        active = first
        if (!enabled()) return AiState.Unsupported
        val firstState = first.state()
        if (firstState == AiState.Ready) return firstState
        for (other in providers.drop(1)) {
            if (other.state() == AiState.Ready) {
                active = other
                return AiState.Ready
            }
        }
        return firstState
    }

    override fun download(): Flow<Long> = providers.first().download()

    override fun generate(prompt: String): Flow<String> = active.generate(prompt)
}

/** Apaga do aparelho o que o modelo local (removido do app) deixou: o arquivo baixado e o cache do motor. */
internal fun deleteLocalModelFiles(models: File?, cache: File) {
    models?.deleteRecursively()
    cache.listFiles { file -> file.name.endsWith(".xnnpack_cache") }?.forEach { it.delete() }
}

object Assistant {
    private const val KEY = "assistant_enabled"
    private const val DOWNLOAD_KEY = "local_model_download_id"
    private const val CLEANED_KEY = "local_model_removed"

    private fun prefs(context: Context) = context.getSharedPreferences("chamados", Context.MODE_PRIVATE)

    /** O assistente está ligado nas configurações (padrão: ligado). */
    fun enabled(context: Context): Boolean = prefs(context).getBoolean(KEY, true)

    fun setEnabled(context: Context, value: Boolean) {
        prefs(context).edit().putBoolean(KEY, value).apply()
    }

    /**
     * O provedor a usar: primeiro a IA do sistema (o app é Android 8+, como a biblioteca) e, se ela não estiver
     * pronta, a IA na nuvem com a chave da própria pessoa (desligada até ela ligar; #144).
     */
    fun provider(context: Context): AnswerProvider {
        val cloud = CloudAnswerProvider({ CloudAi.config(context) })
        return ChainProvider(listOf(AicoreProvider, cloud)) { enabled(context) }
    }

    /**
     * Quem baixou o modelo local numa versão anterior (1,6 GB) o perde aqui, uma vez: cancela o download
     * pendente, apaga o arquivo e o cache do motor, e esquece a marca.
     */
    fun removeLeftoverLocalModel(context: Context) {
        val prefs = prefs(context)
        if (prefs.getBoolean(CLEANED_KEY, false)) return
        val pending = prefs.getLong(DOWNLOAD_KEY, -1L)
        if (pending != -1L) {
            runCatching { (context.getSystemService(Context.DOWNLOAD_SERVICE) as DownloadManager).remove(pending) }
        }
        deleteLocalModelFiles(context.getExternalFilesDir(null)?.let { File(it, "models") }, context.cacheDir)
        prefs.edit().remove(DOWNLOAD_KEY).putBoolean(CLEANED_KEY, true).apply()
    }

    /** O estado só da IA do sistema (para as configurações). */
    suspend fun systemState(): AiState = AicoreProvider.state()

}

/** A Prompt API do ML Kit GenAI (Gemini Nano pelo AICore); o pedido leva menos de 4000 tokens. */
private object AicoreProvider : AnswerProvider {
    private val model: GenerativeModel by lazy { Generation.getClient() }

    override suspend fun state(): AiState = try {
        when (model.checkStatus()) {
            FeatureStatus.AVAILABLE -> AiState.Ready
            FeatureStatus.DOWNLOADABLE -> AiState.Downloadable
            FeatureStatus.DOWNLOADING -> AiState.Downloading
            else -> AiState.Unsupported
        }
    } catch (error: Exception) {
        AiState.Unsupported
    }

    override fun download(): Flow<Long> = flow {
        model.download().collect { status ->
            when (status) {
                is DownloadStatus.DownloadProgress -> emit(status.totalBytesDownloaded)
                is DownloadStatus.DownloadFailed -> throw status.e
                else -> {}
            }
        }
    }

    override fun generate(prompt: String): Flow<String> = flow {
        // Pouca criatividade: o texto é redação de um chamado, não literatura.
        val request = GenerateContentRequest.Builder(TextPart(prompt)).apply {
            temperature = 0.3f
            maxOutputTokens = 700
        }.build()
        var text = ""
        model.generateContentStream(request).collect { chunk ->
            text += chunk.candidates.firstOrNull()?.text.orEmpty()
            emit(text)
        }
    }
}

/** Uma mensagem em português para a falha da IA; nunca inclui o texto do chamado. */
fun Throwable.assistMessage(): String = when {
    this is GenAiException && errorCode == 601 ->
        "O serviço de IA do aparelho (AICore) não respondeu. Atualize o AICore na Play Store e tente de novo."
    this is GenAiException && errorCode == 606 ->
        "A IA do aparelho ainda está sendo preparada. Isso pode levar de minutos a horas; tente mais tarde."
    this is GenAiException -> "A IA do aparelho falhou (código $errorCode). Tente de novo."
    // A IA na nuvem: o Rust já diz o que houve em português (sem a chave nem o texto do chamado).
    this is uniffi.chamados_mobile.ChamadosException.Failed -> detail
    else -> "Não foi possível gerar o texto agora. Tente de novo."
}
