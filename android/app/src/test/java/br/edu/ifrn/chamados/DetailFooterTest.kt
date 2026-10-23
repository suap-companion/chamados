package br.edu.ifrn.chamados

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/** O rodapé do detalhe: o principal e a alternativa de cada situação (lógica pura). */
class DetailFooterTest {
    private fun plan(vararg statuses: String) = footerPlan(statuses.toList())

    @Test
    fun `aberto e reaberto assumem e iniciam, com a alternativa de so assumir`() {
        for (status in listOf("Aberto", "Reaberto")) {
            assertEquals(FooterPlan(FooterStep.AssumeAndStart, FooterStep.Assume, canCancel = true), plan(status))
        }
    }

    @Test
    fun `em atendimento resolve ou suspende`() {
        assertEquals(FooterPlan(FooterStep.Resolve, FooterStep.Suspend, canCancel = true), plan("Em atendimento"))
        assertEquals(FooterStep.Resolve, plan("Em atendimento", "SLA estourado").primary)
    }

    @Test
    fun `suspenso retoma o atendimento, sem alternativa`() {
        assertEquals(FooterPlan(FooterStep.Resume, null, canCancel = true), plan("Suspenso"))
        assertEquals(FooterPlan(FooterStep.Resume, null, canCancel = true), plan("SUSPENSO"))
    }

    @Test
    fun `resolvido fecha ou reabre e nao cancela`() {
        assertEquals(FooterPlan(FooterStep.CloseTicket, FooterStep.Reopen, canCancel = false), plan("Resolvido"))
    }

    @Test
    fun `fechado e cancelado nao tem rodape`() {
        for (status in listOf("Fechado", "Cancelado")) {
            val closed = plan(status)
            assertTrue(status, closed.isEmpty)
            assertNull(closed.primary)
            assertFalse(closed.canCancel)
        }
    }

    @Test
    fun `sem situacao conhecida oferece o comeco do atendimento`() {
        assertEquals(FooterStep.AssumeAndStart, plan().primary)
    }

    @Test
    fun `cada passo leva a acao certa e so assumir e iniciar tem duas chamadas`() {
        assertEquals(DetailAction.Assume, FooterStep.Assume.action)
        assertEquals(DetailAction.Start, FooterStep.Resume.action)
        assertEquals(DetailAction.Resolve, FooterStep.Resolve.action)
        assertEquals(DetailAction.Suspend, FooterStep.Suspend.action)
        assertEquals(DetailAction.Close, FooterStep.CloseTicket.action)
        assertEquals(DetailAction.Reopen, FooterStep.Reopen.action)
        assertNull(FooterStep.AssumeAndStart.action)
    }

    @Test
    fun `assumir e iniciar diz quando so o primeiro passo deu certo`() {
        assertEquals("Assumir e iniciar: feito.", assumeAndStartMessage(null))
        val partial = assumeAndStartMessage("o SUAP recusou")
        assertTrue(partial.contains("assumido") && partial.contains("o SUAP recusou"))
    }
}
