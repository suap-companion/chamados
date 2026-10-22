package br.edu.ifrn.chamados

import java.io.File
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.flow.toList
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TemporaryFolder

/** A escolha do provedor (provedores falsos, sem aparelho) e a limpeza do modelo local antigo. */
class ChainProviderTest {
    @get:Rule
    val folder = TemporaryFolder()

    private class Fake(private val state: AiState, private val answer: String) : AnswerProvider {
        override suspend fun state(): AiState = state
        override fun download(): Flow<Long> = flowOf(answer.length.toLong())
        override fun generate(prompt: String): Flow<String> = flowOf(answer)
    }

    private fun chain(vararg providers: Fake, enabled: Boolean = true) = ChainProvider(providers.toList()) { enabled }

    private fun ChainProvider.answer(): List<String> = runBlocking { state(); generate("p").toList() }

    @Test
    fun `com um provedor so, ele pronto responde`() {
        val provider = chain(Fake(AiState.Ready, "sistema"))
        assertEquals(AiState.Ready, runBlocking { provider.state() })
        assertEquals(listOf("sistema"), provider.answer())
    }

    @Test
    fun `sem nenhum pronto fica o estado do primeiro`() {
        assertEquals(AiState.Downloadable, runBlocking { chain(Fake(AiState.Downloadable, "a")).state() })
        assertEquals(AiState.Unsupported, runBlocking { chain(Fake(AiState.Unsupported, "a"), Fake(AiState.Downloading, "b")).state() })
    }

    @Test
    fun `o primeiro pronto vence e, se o primeiro nao esta, entra o seguinte pronto`() {
        assertEquals(listOf("a"), chain(Fake(AiState.Ready, "a"), Fake(AiState.Ready, "b")).answer())
        assertEquals(listOf("b"), chain(Fake(AiState.Unsupported, "a"), Fake(AiState.Ready, "b")).answer())
    }

    @Test
    fun `desligado nas configuracoes nada e usado`() {
        assertEquals(AiState.Unsupported, runBlocking { chain(Fake(AiState.Ready, "a"), enabled = false).state() })
    }

    @Test
    fun `o download e sempre o do primeiro provedor`() {
        assertEquals(listOf(1L), runBlocking { chain(Fake(AiState.Downloadable, "a"), Fake(AiState.Ready, "bb")).download().toList() })
    }

    @Test
    fun `a limpeza do modelo local apaga o arquivo e o cache do motor e deixa o resto`() {
        val models = folder.newFolder("models")
        File(models, "Qwen.litertlm").writeText("x")
        File(models, "Qwen.litertlm.download").writeText("y")
        val cache = folder.newFolder("cache")
        File(cache, "Qwen.litertlm_1_2.xnnpack_cache").writeText("c")
        File(cache, "outro.tmp").writeText("fica")

        deleteLocalModelFiles(models, cache)

        assertFalse(models.exists())
        assertFalse(File(cache, "Qwen.litertlm_1_2.xnnpack_cache").exists())
        assertTrue(File(cache, "outro.tmp").exists())
        // Sem pasta de modelos (armazenamento indisponível) nada quebra.
        deleteLocalModelFiles(null, cache)
    }
}
