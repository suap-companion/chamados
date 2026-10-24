package br.edu.ifrn.chamados

import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test

/** A linha de serviço do passo 1: nome, detalhe e o toque que escolhe. */
class ServiceRowTest {
    @get:Rule
    val compose = createComposeRule()

    @Test
    fun it_shows_the_name_and_detail_and_reports_the_tap() {
        var taps = 0
        compose.setContent { ServiceRow("Infraestrutura para o Moodle", "Moodle · #400", selected = false) { taps++ } }
        compose.onNodeWithText("Infraestrutura para o Moodle").assertIsDisplayed()
        compose.onNodeWithText("Moodle · #400").assertIsDisplayed().performClick()
        assertEquals(1, taps)
    }
}
