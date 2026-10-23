package br.edu.ifrn.chamados

import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.assertIsNotEnabled
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test

/** O rodapé do detalhe na tela: os botões da situação, o que cada toque chama e o estado "ocupado". */
class DetailFooterContentTest {
    @get:Rule
    val compose = createComposeRule()

    private val steps = mutableListOf<FooterStep>()
    private var snoozed = 0
    private var cancelled = 0

    private fun show(statuses: List<String>, busy: Boolean = false, message: String? = null) {
        compose.setContent {
            DetailFooterContent(
                plan = footerPlan(statuses), busy = busy, message = message, messageIsError = false,
                onStep = { steps += it }, onSnooze = { snoozed++ }, onCancel = { cancelled++ },
            )
        }
    }

    @Test
    fun an_open_ticket_offers_assume_and_start_assume_only_snooze_and_cancel() {
        show(listOf("Aberto"))
        compose.onNodeWithText("Assumir e iniciar").assertIsDisplayed().performClick()
        compose.onNodeWithText("Só assumir").assertIsDisplayed().performClick()
        compose.onNodeWithText("Adiar").assertIsDisplayed().performClick()
        compose.onNodeWithText("Cancelar chamado").assertIsDisplayed().performClick()
        assertEquals(listOf(FooterStep.AssumeAndStart, FooterStep.Assume), steps)
        assertEquals(1, snoozed)
        assertEquals(1, cancelled)
    }

    @Test
    fun a_resolved_ticket_closes_or_reopens_and_cannot_be_cancelled() {
        show(listOf("Resolvido"))
        compose.onNodeWithText("Fechar e avaliar").assertIsDisplayed()
        compose.onNodeWithText("Reabrir").assertIsDisplayed()
        compose.onNodeWithText("Cancelar chamado").assertDoesNotExist()
    }

    @Test
    fun a_closed_ticket_has_no_footer_and_a_busy_one_blocks_the_actions() {
        show(listOf("Fechado"))
        compose.onNodeWithText("Adiar").assertDoesNotExist()
    }

    @Test
    fun while_an_action_runs_the_buttons_wait_and_the_message_shows() {
        show(listOf("Em atendimento"), busy = true, message = "Resolver: feito.")
        compose.onNodeWithText("Resolver").assertIsNotEnabled()
        compose.onNodeWithText("Suspender").assertIsNotEnabled()
        compose.onNodeWithText("Resolver: feito.").assertIsDisplayed()
    }
}
