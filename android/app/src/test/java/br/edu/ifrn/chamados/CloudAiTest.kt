package br.edu.ifrn.chamados

import kotlinx.coroutines.flow.toList
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

/** A IA na nuvem: a chave nunca aparece inteira, o aviso diz o servidor, e o provedor só fica pronto com chave. */
class CloudAiTest {
    private val config = CloudConfig("openai", "gpt-x", "https://api.exemplo.com/v1", "sk-segredo-1234")

    @Test
    fun `a chave aparece so pelo fim`() {
        assertEquals("••••1234", maskKey("sk-segredo-1234"))
        assertEquals("••••1234", maskKey("  sk-segredo-1234 \n"))
        assertFalse(maskKey("sk-segredo-1234").contains("segredo"))
    }

    @Test
    fun `o aviso diz o servidor informado ou o padrao do provedor`() {
        assertEquals("api.exemplo.com", hostOf("https://api.exemplo.com/v1", "https://api.openai.com/v1"))
        assertEquals("api.openai.com", hostOf("", "https://api.openai.com/v1"))
        assertEquals("api.openai.com", hostOf(null, "https://api.openai.com/v1"))
        assertEquals("api.openai.com", hostOf("  ", "https://api.openai.com/v1"))
        // Endereço que não dá para ler: cai no padrão em vez de quebrar.
        assertEquals("https://api.openai.com/v1", hostOf("nao e um endereco", "https://api.openai.com/v1"))
    }

    @Test
    fun `sem configuracao a IA na nuvem nao esta pronta e com ela esta`() {
        assertEquals(AiState.Unsupported, runBlocking { CloudAnswerProvider({ null }).state() })
        assertEquals(AiState.Ready, runBlocking { CloudAnswerProvider({ config }).state() })
        assertEquals(emptyList<Long>(), runBlocking { CloudAnswerProvider({ config }).download().toList() })
    }

    @Test
    fun `o pedido leva a configuracao e o texto e a resposta volta de uma vez`() {
        val seen = mutableListOf<Pair<CloudConfig, String>>()
        val provider = CloudAnswerProvider({ config }) { cloud, prompt -> seen += cloud to prompt; "Olá" }
        assertEquals(listOf("Olá"), runBlocking { provider.generate("escreva").toList() })
        assertEquals(listOf(config to "escreva"), seen)
    }

    @Test
    fun `desligada no meio do caminho o pedido falha sem chamar o provedor`() {
        var called = false
        val provider = CloudAnswerProvider({ null }) { _, _ -> called = true; "x" }
        val failure = runCatching { runBlocking { provider.generate("p").toList() } }.exceptionOrNull()
        assertTrue(failure is IllegalStateException)
        assertFalse(called)
    }

    @Test
    fun `na cadeia, a IA do sistema vem antes e a nuvem entra se ela nao estiver pronta`() {
        val system = object : AnswerProvider {
            override suspend fun state() = AiState.Unsupported
            override fun download() = kotlinx.coroutines.flow.emptyFlow<Long>()
            override fun generate(prompt: String) = kotlinx.coroutines.flow.flowOf("sistema")
        }
        val cloud = CloudAnswerProvider({ config }) { _, _ -> "nuvem" }
        val chain = ChainProvider(listOf(system, cloud)) { true }
        assertEquals(AiState.Ready, runBlocking { chain.state() })
        assertEquals(listOf("nuvem"), runBlocking { chain.generate("p").toList() })
        // Sem chave, a nuvem não vale e o estado é o da IA do sistema.
        val noKey = ChainProvider(listOf(system, CloudAnswerProvider({ null }))) { true }
        assertEquals(AiState.Unsupported, runBlocking { noKey.state() })
    }
}
