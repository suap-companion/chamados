package br.edu.ifrn.chamados

import androidx.compose.foundation.layout.Column
import androidx.compose.ui.Modifier
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.isToggleable
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import androidx.test.core.app.ApplicationProvider
import org.junit.After
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test

/** Ligar a IA na nuvem exige ler o aviso (para onde o texto vai) e confirmar; desligar é direto. */
class CloudSectionTest {
    @get:Rule
    val compose = createComposeRule()

    private val app = ApplicationProvider.getApplicationContext<android.app.Application>()

    @After
    fun cleanUp() {
        CloudAi.setEnabled(app, false)
    }

    @Test
    fun turning_it_on_asks_first_and_cancelling_leaves_it_off() {
        CloudAi.setEnabled(app, false)
        compose.setContent { Column(Modifier.verticalScroll(rememberScrollState())) { AssistantSection(onDocuments = {}) } }
        compose.onNodeWithText("Desligada").assertIsDisplayed()

        // O segundo interruptor é o da nuvem (o primeiro liga e desliga o assistente).
        compose.onAllNodes(isToggleable())[1].performClick()
        compose.onNodeWithText("Ligar a IA na nuvem?").assertIsDisplayed()
        compose.onNodeWithText("Cancelar").performClick()
        compose.onNodeWithText("Desligada").assertIsDisplayed()
        assertFalse(CloudAi.enabled(app))

        compose.onAllNodes(isToggleable())[1].performClick()
        compose.onNodeWithText("Entendi, ligar").performClick()
        compose.onNodeWithText("Ligada").assertIsDisplayed()
        assertTrue(CloudAi.enabled(app))

        // Desligar não pergunta nada.
        compose.onAllNodes(isToggleable())[1].performClick()
        compose.onNodeWithText("Desligada").assertIsDisplayed()
        assertFalse(CloudAi.enabled(app))
    }
}
