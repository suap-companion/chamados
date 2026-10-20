package br.edu.ifrn.chamados

import android.content.Context
import com.google.ai.edge.litertlm.Backend
import com.google.ai.edge.litertlm.Content
import com.google.ai.edge.litertlm.ConversationConfig
import com.google.ai.edge.litertlm.Engine
import com.google.ai.edge.litertlm.EngineConfig
import com.google.ai.edge.litertlm.Message
import com.google.ai.edge.litertlm.SamplerConfig
import java.io.File
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.emptyFlow
import kotlinx.coroutines.flow.flow
import kotlinx.coroutines.flow.flowOn
import kotlinx.coroutines.withContext

/**
 * O modelo local de reserva (LiteRT-LM com o Qwen2.5 baixado pelo usuário). O motor ocupa ~2 GB de
 * memória: fica carregado enquanto o diálogo de texto está aberto (os refinamentos ficam rápidos) e é
 * fechado por [LocalLlm.release] quando o diálogo some.
 */
object LocalLlm : AnswerProvider {
    private var engine: Engine? = null
    private var appContext: Context? = null

    fun attach(context: Context) {
        appContext = context.applicationContext
    }

    private fun files(): LocalModelFiles? =
        appContext?.getExternalFilesDir(null)?.let { LocalModelFiles(File(it, "models")) }

    override suspend fun state(): AiState =
        if (files()?.isInstalled() == true) AiState.Ready else AiState.Unsupported

    override fun download(): Flow<Long> = emptyFlow()

    override fun generate(prompt: String): Flow<String> = flow {
        val running = ensureEngine()
        // Pouca criatividade: o texto é a redação de um chamado.
        val config = ConversationConfig(samplerConfig = SamplerConfig(40, 0.95, 0.3, 0))
        running.createConversation(config).use { conversation ->
            var text = ""
            conversation.sendMessageAsync(prompt).collect { message ->
                text = merge(text, message.text())
                emit(text)
            }
        }
    }.flowOn(Dispatchers.Default)

    /** Carrega o motor (pode levar uns 10 segundos na primeira vez); reaproveita o que já está carregado. */
    private suspend fun ensureEngine(): Engine = withContext(Dispatchers.Default) {
        engine ?: run {
            val context = checkNotNull(appContext) { "LocalLlm sem contexto" }
            val model = checkNotNull(files()?.model?.takeIf { it.isFile }) { "Modelo local não instalado" }
            val config = EngineConfig(
                modelPath = model.absolutePath,
                backend = Backend.CPU(),
                cacheDir = context.cacheDir.path,
            )
            Engine(config).also {
                it.initialize()
                engine = it
            }
        }
    }

    /** Libera a memória do modelo. */
    fun release() {
        engine?.close()
        engine = null
    }
}

/** O texto de uma mensagem do modelo. */
private fun Message.text(): String =
    contents.contents.filterIsInstance<Content.Text>().joinToString("") { it.text }

/**
 * O fluxo do motor entrega pedaços do texto; se algum dia entregar o texto inteiro até ali, não duplica:
 * um pedaço que começa com o que já temos substitui o acumulado.
 */
internal fun merge(current: String, chunk: String): String =
    if (current.isNotEmpty() && chunk.length > current.length && chunk.startsWith(current)) chunk else current + chunk
