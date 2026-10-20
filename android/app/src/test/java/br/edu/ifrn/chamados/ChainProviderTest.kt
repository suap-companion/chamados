package br.edu.ifrn.chamados

import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.flow.toList
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Test

/** A escolha entre a IA do sistema e o modelo local (provedores falsos, sem aparelho). */
class ChainProviderTest {
    private class Fake(private val state: AiState, private val answer: String) : AnswerProvider {
        override suspend fun state(): AiState = state
        override fun download(): Flow<Long> = flowOf(answer.length.toLong())
        override fun generate(prompt: String): Flow<String> = flowOf(answer)
    }

    private fun chain(system: AiState, local: AiState, enabled: Boolean = true) =
        ChainProvider(Fake(system, "sistema"), Fake(local, "local")) { enabled }

    private fun ChainProvider.answer(): List<String> = runBlocking { state(); generate("p").toList() }

    @Test
    fun `a IA do sistema pronta vence o modelo local`() {
        val provider = chain(AiState.Ready, AiState.Ready)
        assertEquals(AiState.Ready, runBlocking { provider.state() })
        assertEquals(listOf("sistema"), provider.answer())
    }

    @Test
    fun `sem a IA do sistema entra o modelo local instalado`() {
        for (system in listOf(AiState.Unsupported, AiState.Downloadable, AiState.Downloading)) {
            val provider = chain(system, AiState.Ready)
            assertEquals(AiState.Ready, runBlocking { provider.state() })
            assertEquals(listOf("local"), provider.answer())
        }
    }

    @Test
    fun `sem nenhum dos dois fica o estado da IA do sistema`() {
        assertEquals(AiState.Downloadable, runBlocking { chain(AiState.Downloadable, AiState.Unsupported).state() })
        assertEquals(AiState.Unsupported, runBlocking { chain(AiState.Unsupported, AiState.Unsupported).state() })
    }

    @Test
    fun `desligado nas configuracoes nada e usado`() {
        assertEquals(AiState.Unsupported, runBlocking { chain(AiState.Ready, AiState.Ready, enabled = false).state() })
    }

    @Test
    fun `o download e sempre o da IA do sistema`() {
        assertEquals(listOf(7L), runBlocking { chain(AiState.Downloadable, AiState.Ready).download().toList() })
    }
}
