package br.edu.ifrn.chamados

import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import java.time.LocalDate
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test

/** O topo da tela Hoje: a data, os três contadores e o que cada toque abre. */
class TodayHeaderTest {
    @get:Rule
    val compose = createComposeRule()

    private val taps = mutableListOf<String>()

    private fun show(counters: TodayCounters) {
        compose.setContent {
            TodayHeader(
                date = LocalDate.of(2026, 10, 23), counters = counters,
                onAttending = { taps += "atendimento" }, onOverdue = { taps += "sla" }, onReturned = { taps += "voltou" },
            )
        }
    }

    @Test
    fun it_shows_the_long_date_and_the_three_counters() {
        show(TodayCounters(attending = 3, overdue = 2, returned = 1))
        compose.onNodeWithText("Sexta-feira, 23 de outubro de 2026").assertIsDisplayed()
        compose.onNodeWithText("3").assertIsDisplayed()
        compose.onNodeWithText("2").assertIsDisplayed()
        compose.onNodeWithText("1").assertIsDisplayed()
    }

    @Test
    fun tapping_each_counter_opens_its_own_list() {
        show(TodayCounters(attending = 3, overdue = 2, returned = 1))
        compose.onNodeWithText("Em atendimento").performClick()
        compose.onNodeWithText("SLA estourado").performClick()
        compose.onNodeWithText("Voltou do adiamento").performClick()
        assertEquals(listOf("atendimento", "sla", "voltou"), taps)
    }
}
