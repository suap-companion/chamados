package br.edu.ifrn.chamados

import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test

/** O cartão de fatos do cabeçalho: serviço, interessado, responsável com a ação certa e o prazo do SLA. */
class HeaderFactsContentTest {
    @get:Rule
    val compose = createComposeRule()

    private var taps = 0

    private fun show(facts: HeaderFacts, closed: Boolean = false) {
        compose.setContent { HeaderFactsContent(facts, closed) { taps++ } }
    }

    @Test
    fun without_an_owner_it_says_nobody_and_offers_to_assume() {
        show(HeaderFacts("Infraestrutura", "ZL", "Wagner", null, null, false))
        compose.onNodeWithText("Infraestrutura · ZL").assertIsDisplayed()
        compose.onNodeWithText("ninguém").assertIsDisplayed()
        compose.onNodeWithText("Assumir").assertIsDisplayed().performClick()
        assertEquals(1, taps)
    }

    @Test
    fun with_an_owner_it_shows_the_name_and_offers_to_assign() {
        show(HeaderFacts("Infraestrutura", null, "Wagner", "Kelson", "24/10/2026 17:00", false))
        compose.onNodeWithText("Kelson").assertIsDisplayed()
        compose.onNodeWithText("Atribuir").assertIsDisplayed()
        compose.onNodeWithText("24/10/2026 17:00").assertIsDisplayed()
    }

    @Test
    fun an_overdue_deadline_is_marked_and_a_closed_ticket_has_no_owner_action() {
        show(HeaderFacts(null, null, null, "Kelson", "22/10/2026 09:30", true), closed = true)
        compose.onNodeWithText("22/10/2026 09:30 (estourado)").assertIsDisplayed()
        compose.onNodeWithText("Atribuir").assertDoesNotExist()
        compose.onNodeWithText("Assumir").assertDoesNotExist()
    }
}
