package br.edu.ifrn.chamados

import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import uniffi.chamados_mobile.Queue

/** Quando a lista da aba vem inteira e não pagina (lógica pura). */
class PagesInOneGoTest {
    @Test
    fun `adiados e arquivados vem de uma vez, menos na aba A fechar`() {
        for (mode in listOf(ListMode.Snoozed, ListMode.Archived)) {
            assertTrue(pagesInOneGo(Queue.SUPPORT, mode))
            assertTrue(pagesInOneGo(Queue.MINE, mode))
            assertFalse(pagesInOneGo(Queue.TO_CLOSE, mode))
        }
    }

    @Test
    fun `a lista de ativos sempre pagina`() {
        for (queue in Queue.entries) assertFalse(pagesInOneGo(queue, ListMode.Normal))
    }
}
