package br.edu.ifrn.chamados

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test
import uniffi.chamados_mobile.Queue

/** O filtro "Atribuídos a mim" só existe na fila de suporte (lógica pura). */
class AssignmentFilterTest {
    @Test
    fun `ligado na fila de suporte pede os atribuidos a mim`() {
        assertEquals("mim", assignmentFilter(Queue.SUPPORT, true))
    }

    @Test
    fun `desligado nao filtra`() {
        assertNull(assignmentFilter(Queue.SUPPORT, false))
    }

    @Test
    fun `nas outras abas o filtro nunca vai ao SUAP mesmo ligado`() {
        assertNull(assignmentFilter(Queue.MINE, true))
        assertNull(assignmentFilter(Queue.TO_CLOSE, true))
    }
}
