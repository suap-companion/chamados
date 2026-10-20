package br.edu.ifrn.chamados

import androidx.compose.foundation.layout.Column
import androidx.compose.material3.Text
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onAllNodesWithText
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.flowOf
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import uniffi.chamados_mobile.DraftKind
import uniffi.chamados_mobile.EntryKind
import uniffi.chamados_mobile.ThreadItem
import uniffi.chamados_mobile.TicketField
import uniffi.chamados_mobile.TicketView

/**
 * "Sugerir com IA" de ponta a ponta no aparelho, com a biblioteca nativa de verdade (prompt, limpeza e modelo
 * pronto vêm do Rust) e um provedor de IA falso no lugar do Gemini Nano.
 */
class AssistButtonTest {
    @get:Rule
    val compose = createComposeRule()

    /** Um provedor falso: o estado que se quer, a resposta que se quer e os pedidos que recebeu. */
    private class FakeProvider(private val state: AiState, private val answer: String = "") : AnswerProvider {
        val prompts = mutableListOf<String>()
        override suspend fun state(): AiState = state
        override fun download(): Flow<Long> = flowOf()
        override fun generate(prompt: String): Flow<String> {
            prompts += prompt
            return flowOf(answer)
        }
    }

    private fun entry(kind: EntryKind, key: String, body: String) =
        ThreadItem(kind = kind, date = key, sortKey = key, author = "Ana", photo = null, body = body)

    private val ticket = TicketView(
        id = "7", title = "Chamado 7", heading = "Moodle | Atualização", statuses = listOf("Aberto"),
        fields = listOf(TicketField("Descrição", "Atualizar o Moodle")),
        thread = listOf(entry(EntryKind.COMMENT, "2026-10-13 10:00:00", "Já subiram?")),
        notes = listOf(entry(EntryKind.NOTE, "2026-10-12 11:00:00", "SEGREDO da equipe")),
        canSeeNotes = true, attachments = emptyList(), photo = null, url = "",
    )

    private fun show(kind: DraftKind, provider: FakeProvider, initial: String = "") {
        compose.setContent {
            var text by remember { mutableStateOf(initial) }
            Column {
                Text("campo: $text")
                AssistButton(kind, ticket, null, text, provider = provider) { text = it }
            }
        }
    }

    @Test
    fun a_ready_AI_fills_the_field_with_the_cleaned_draft_and_offers_refinements() {
        val provider = FakeProvider(AiState.Ready, "**Rascunho** de comentário")
        show(DraftKind.COMMENT, provider)
        compose.onNodeWithText("Sugerir com IA").performClick()
        compose.waitUntil(5_000) { compose.onAllNodesWithTextCount("campo: Rascunho de comentário") == 1 }
        compose.onNodeWithText("Mais curto").assertIsDisplayed()

        compose.onNodeWithText("Mais curto").performClick()
        compose.waitUntil(5_000) { provider.prompts.size == 2 }
        assertTrue(provider.prompts[1].contains("mais curto"))
        assertTrue(provider.prompts[1].contains("Rascunho de comentário"))
    }

    @Test
    fun without_AI_the_field_gets_a_template_with_a_placeholder_and_a_notice() {
        val provider = FakeProvider(AiState.Unsupported)
        show(DraftKind.CANCEL, provider, initial = "duplicado")
        compose.onNodeWithText("Sugerir com IA").performClick()
        compose.waitUntil(5_000) { compose.onAllNodesWithTextCount("campo: Chamado cancelado: duplicado.") == 1 }
        compose.onNodeWithText("Este aparelho não tem IA disponível", substring = true).assertIsDisplayed()
        assertTrue(provider.prompts.isEmpty())
    }

    @Test
    fun internal_notes_reach_the_model_only_for_internal_texts() {
        val publicProvider = FakeProvider(AiState.Ready, "ok")
        show(DraftKind.COMMENT, publicProvider)
        compose.onNodeWithText("Sugerir com IA").performClick()
        compose.waitUntil(5_000) { publicProvider.prompts.size == 1 }
        assertFalse(publicProvider.prompts[0].contains("SEGREDO"))
        assertTrue(publicProvider.prompts[0].contains("Já subiram?"))
    }

    @Test
    fun an_internal_text_may_see_the_internal_notes() {
        val internalProvider = FakeProvider(AiState.Ready, "ok")
        show(DraftKind.INTERNAL_NOTE, internalProvider)
        compose.onNodeWithText("Sugerir com IA").performClick()
        compose.waitUntil(5_000) { internalProvider.prompts.size == 1 }
        assertTrue(internalProvider.prompts[0].contains("SEGREDO"))
        assertEquals(1, internalProvider.prompts.size)
    }

    private fun androidx.compose.ui.test.junit4.ComposeContentTestRule.onAllNodesWithTextCount(text: String): Int =
        onAllNodesWithText(text).fetchSemanticsNodes().size
}

