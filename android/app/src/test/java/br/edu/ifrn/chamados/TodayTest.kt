package br.edu.ifrn.chamados

import java.time.LocalDate
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import uniffi.chamados_mobile.Queue
import uniffi.chamados_mobile.SnoozeItem
import uniffi.chamados_mobile.TicketSummary

/** O que a tela Hoje monta a partir das listas (lógica pura). */
class TodayTest {
    private fun ticket(id: String, status: String?) = TicketSummary(id = id, status = status, subject = "Assunto $id", photo = null)

    private fun snooze(id: String, due: Boolean) = SnoozeItem(id = id, until = 0u, due = due)

    @Test
    fun `contadores saem das listas e cada chamado aparece numa secao so`() {
        val data = todayData(
            mine = listOf(ticket("1", "Em atendimento"), ticket("2", "Aberto"), ticket("3", "Em atendimento"), ticket("4", "Resolvido")),
            snoozed = listOf(ticket("5", "Em atendimento"), ticket("6", "Aberto")),
            overdue = listOf(ticket("3", "Em atendimento"), ticket("9", "Aberto")),
            resolved = listOf(ticket("4", "Resolvido"), ticket("7", "Resolvido")),
            snoozes = mapOf("5" to snooze("5", due = true), "6" to snooze("6", due = false)),
        )
        // SLA estourado primeiro entre os abertos; o adiado que voltou fica só em "voltou".
        assertEquals(listOf("3", "1", "2"), data.open.map { it.id })
        assertEquals(listOf("5"), data.returned.map { it.id })
        assertEquals(listOf("4", "7"), data.resolved.map { it.id }.sorted())
        assertEquals(TodayCounters(attending = 3, overdue = 2, returned = 1), data.counters)
        assertEquals(setOf("3", "9"), data.overdueIds)
    }

    @Test
    fun `um que voltou nao se repete entre os abertos e finalizados na lista ativa vao para resolvidos`() {
        val data = todayData(
            mine = listOf(ticket("1", "Aberto"), ticket("2", "Fechado"), ticket("1", "Aberto")),
            snoozed = listOf(ticket("1", "Aberto")),
            overdue = emptyList(),
            resolved = emptyList(),
            snoozes = mapOf("1" to snooze("1", due = true)),
        )
        assertTrue(data.open.isEmpty())
        assertEquals(listOf("1"), data.returned.map { it.id })
        assertEquals(listOf("2"), data.resolved.map { it.id })
    }

    @Test
    fun `sem nada tudo vem vazio`() {
        val data = todayData(emptyList(), emptyList(), emptyList(), emptyList(), emptyMap())
        assertEquals(TodayCounters(0, 0, 0), data.counters)
        assertTrue(data.open.isEmpty() && data.returned.isEmpty() && data.resolved.isEmpty())
    }

    @Test
    fun `a situacao diz se esta em atendimento ou terminou`() {
        assertTrue(ticket("1", "Em atendimento").isAttending())
        assertFalse(ticket("1", "Aberto").isAttending())
        assertFalse(ticket("1", null).isAttending())
        for (status in listOf("Resolvido", "FECHADO", "Cancelado")) assertTrue(status, ticket("1", status).isFinished())
        assertFalse(ticket("1", "Suspenso").isFinished())
        assertFalse(ticket("1", null).isFinished())
    }

    @Test
    fun `a data sai por extenso em portugues`() {
        assertEquals("sexta-feira, 23 de outubro de 2026", longDate(LocalDate.of(2026, 10, 23)))
        assertEquals("domingo, 1 de março de 2026", longDate(LocalDate.of(2026, 3, 1)))
    }

    @Test
    fun `cada aba da barra leva a sua fila e Hoje a nenhuma`() {
        assertEquals(null, HomeTab.Today.queue())
        assertEquals(Queue.MINE, HomeTab.Mine.queue())
        assertEquals(Queue.SUPPORT, HomeTab.Support.queue())
        assertEquals(Queue.TO_CLOSE, HomeTab.ToClose.queue())
    }

    @Test
    fun `o filtro de SLA so vale na fila de suporte`() {
        assertTrue(slaFilter(Queue.SUPPORT, true))
        assertFalse(slaFilter(Queue.SUPPORT, false))
        assertFalse(slaFilter(Queue.MINE, true))
        assertFalse(slaFilter(Queue.TO_CLOSE, true))
    }
}
