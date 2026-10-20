package br.edu.ifrn.chamados

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotEquals
import org.junit.Test

/** A cor de cada situação na etiqueta (lógica pura): uma cor por sentido, sem diferenciar maiúsculas. */
class StatusColorsTest {
    @Test
    fun `situacoes do mesmo sentido tem a mesma cor`() {
        assertEquals(statusColors("Resolvido"), statusColors("Fechado"))
        assertEquals(statusColors("Em atendimento"), statusColors("Reaberto"))
        assertEquals(statusColors("SUSPENSO"), statusColors("suspenso"))
    }

    @Test
    fun `situacoes de sentidos diferentes tem cores diferentes`() {
        val distinct = listOf("Aberto", "Em atendimento", "Suspenso", "Resolvido", "Cancelado").map { statusColors(it) }
        assertEquals(distinct.size, distinct.toSet().size)
        assertNotEquals(statusColors("Aberto"), statusColors("Suspenso"))
    }

    @Test
    fun `situacao desconhecida usa a cor padrao do aberto`() {
        assertEquals(statusColors("Aberto"), statusColors("Qualquer coisa"))
    }
}
