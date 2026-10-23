package br.edu.ifrn.chamados

import java.time.LocalDateTime
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import uniffi.chamados_mobile.TicketField
import uniffi.chamados_mobile.TicketView

/** O que o cabeçalho do detalhe extrai dos campos da página do chamado (lógica pura). */
class HeaderFactsTest {
    private val now = LocalDateTime.of(2026, 10, 23, 12, 0)

    private fun view(vararg fields: Pair<String, String>, heading: String? = "1.2 Gestão | Infraestrutura para o Moodle") = TicketView(
        id = "7", title = "Chamado 7", heading = heading, statuses = listOf("Em atendimento"),
        fields = fields.map { TicketField(it.first, it.second) }, thread = emptyList(), notes = emptyList(),
        canSeeNotes = false, attachments = emptyList(), photo = null, url = "",
    )

    @Test
    fun `serviço e campus vem do cabeçalho e do centro de atendimento`() {
        val facts = headerFacts(view("Centro de atendimento:" to "Central de Serviços DEAD /ZL"), now)
        assertEquals("Infraestrutura para o Moodle", facts.service)
        assertEquals("ZL", facts.campus)
        assertEquals("Infraestrutura para o Moodle · ZL", facts.serviceLine())
    }

    @Test
    fun `sem campus ou sem cabeçalho aparece so o que existe`() {
        assertEquals("Infraestrutura para o Moodle", headerFacts(view("Centro de atendimento" to "Central de Serviços DEAD"), now).serviceLine())
        assertNull(headerFacts(view("Centro de atendimento" to "Central DEAD"), now).campus)
        // Sem cabeçalho cai no título; sem nada, não há linha.
        assertEquals("Chamado 7", headerFacts(view(heading = null), now).service)
        assertNull(HeaderFacts(null, null, null, null, null, false).serviceLine())
    }

    @Test
    fun `interessado e responsavel saem dos campos e ninguem assumiu quando nao ha atendente`() {
        val assigned = headerFacts(view("Interessado:" to "Wagner", "Atendente:" to "Kelson (2080882)"), now)
        assertEquals("Wagner", assigned.interested)
        assertEquals("Kelson (2080882)", assigned.owner)
        assertNull(headerFacts(view("Interessado" to "Wagner"), now).owner)
        assertNull(headerFacts(view("Atendente" to "   "), now).owner)
    }

    @Test
    fun `o prazo do SLA no futuro mostra a data sem o faltam e nao esta estourado`() {
        val facts = headerFacts(view("Data limite:" to "24/10/2026 17:00:00 (Faltam 1 dia)"), now)
        assertEquals("24/10/2026 17:00:00", facts.deadline)
        assertFalse(facts.overdue)
    }

    @Test
    fun `o prazo que passou sem o faltam esta estourado`() {
        val facts = headerFacts(view("Data limite" to "22/10/2026 09:30"), now)
        assertEquals("22/10/2026 09:30", facts.deadline)
        assertTrue(facts.overdue)
    }

    @Test
    fun `prazo ilegivel ou ausente nao quebra nem estoura`() {
        assertFalse(headerFacts(view("Data limite" to "amanhã"), now).overdue)
        assertEquals("amanhã", headerFacts(view("Data limite" to "amanhã"), now).deadline)
        assertNull(headerFacts(view(), now).deadline)
    }

    @Test
    fun `a data aceita as formas do SUAP`() {
        assertEquals(LocalDateTime.of(2026, 10, 23, 17, 0, 0), parseDeadline("23/10/2026 17:00:00 (Faltam 2 horas)"))
        assertEquals(LocalDateTime.of(2026, 10, 23, 17, 0), parseDeadline("23/10/2026 17:00"))
        assertEquals(LocalDateTime.of(2026, 10, 23, 17, 0), parseDeadline("23/10/2026 às 17:00"))
        assertNull(parseDeadline("sem data"))
        // Hora impossível: não é uma data válida.
        assertNull(parseDeadline("23/10/2026 99:99"))
    }
}
