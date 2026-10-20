package br.edu.ifrn.chamados

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
 * Escolhe o provedor na hora: a IA do sistema, se estiver pronta; senão o modelo local, se estiver instalado;
 * senão fica o estado da IA do sistema (para o app oferecer o download ou o modelo pronto). Desligado nas
 * configurações, nenhum dos dois é usado.
 */
class ChainProvider(
    private val primary: AnswerProvider,
    private val secondary: AnswerProvider,
    private val enabled: () -> Boolean,
) : AnswerProvider {
    private var active: AnswerProvider = primary

    override suspend fun state(): AiState {
        if (!enabled()) return AiState.Unsupported
        val first = primary.state()
        if (first == AiState.Ready) {
            active = primary
            return first
        }
        if (secondary.state() == AiState.Ready) {
            active = secondary
            return AiState.Ready
        }
        active = primary
        return first
    }

    override fun download(): Flow<Long> = primary.download()

    override fun generate(prompt: String): Flow<String> = active.generate(prompt)
}

object Assistant {
    private const val KEY = "assistant_enabled"

    private fun prefs(context: Context) = context.getSharedPreferences("chamados", Context.MODE_PRIVATE)

    /** O assistente está ligado nas configurações (padrão: ligado). */
    fun enabled(context: Context): Boolean = prefs(context).getBoolean(KEY, true)

    fun setEnabled(context: Context, value: Boolean) {
        prefs(context).edit().putBoolean(KEY, value).apply()
    }

    /** O provedor a usar: IA do sistema (o app é Android 8+, como a biblioteca) e, de reserva, o modelo local. */
    fun provider(context: Context): AnswerProvider {
        LocalLlm.attach(context)
        return ChainProvider(AicoreProvider, LocalLlm) { enabled(context) }
    }

    /** O estado só da IA do sistema (para as configurações). */
    suspend fun systemState(): AiState = AicoreProvider.state()

    /** Libera a memória do modelo local. */
    fun release() = LocalLlm.release()
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
    else -> "Não foi possível gerar o texto agora. Tente de novo."
}
